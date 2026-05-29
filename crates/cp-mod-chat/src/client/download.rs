//! Tuwunel binary download and extraction.
//!
//! Downloads the pinned Tuwunel release from GitHub on first run and
//! decompresses it with the system `zstd` command. The binary is
//! installed to `~/.context-pilot/bin/tuwunel` and reused across all
//! projects on the same machine.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::server;

/// Pinned Tuwunel release version shipped with this Context Pilot version.
const TUWUNEL_VERSION: &str = "v1.5.1";

/// GitHub release URL template for the Tuwunel binary (zstd-compressed).
///
/// `VERSION_PLACEHOLDER` and `ARCH_PLACEHOLDER` are filled at runtime.
/// We use the statically-linked GNU build for maximum portability.
const TUWUNEL_URL_TEMPLATE: &str = "https://github.com/matrix-construct/tuwunel/releases/download/VERSION_PLACEHOLDER/VERSION_PLACEHOLDER-release-all-ARCH_PLACEHOLDER-linux-gnu-tuwunel.zst";

/// Placeholder token for the version in [`TUWUNEL_URL_TEMPLATE`].
const VERSION_PLACEHOLDER: &str = "VERSION_PLACEHOLDER";

/// Placeholder token for the architecture in [`TUWUNEL_URL_TEMPLATE`].
const ARCH_PLACEHOLDER: &str = "ARCH_PLACEHOLDER";

/// How long to cache a download failure before retrying.
const DOWNLOAD_RETRY_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Ensure the Tuwunel binary exists at `~/.context-pilot/bin/tuwunel`.
///
/// If absent, downloads the pinned release from GitHub and decompresses
/// it with `zstd`. The download is ~26 MB (compressed) / ~87 MB (binary).
/// This runs once per machine; subsequent calls are a no-op.
///
/// Download failures (HTTP 404, network errors) are cached in a marker
/// file for 24 hours to avoid wasting 2–5 s on every boot retrying a
/// request that is known to fail.
///
/// # Errors
///
/// Returns a description if the download fails, `zstd` is missing, or
/// decompression fails.
pub(crate) fn ensure_binary() -> Result<(), String> {
    let bin_path = server::binary_path().ok_or("Cannot determine home directory for Tuwunel binary")?;
    if bin_path.exists() {
        return Ok(());
    }

    // Check if a recent download attempt already failed — skip the HTTP
    // roundtrip entirely until the retry interval expires.
    let marker = download_failure_marker(&bin_path);
    if let Some(reason) = is_download_recently_failed(&marker) {
        return Err(reason);
    }

    let bin_dir = bin_path.parent().ok_or("Invalid binary path")?;
    std::fs::create_dir_all(bin_dir).map_err(|e| format!("Cannot create {}: {e}", bin_dir.display()))?;

    log::info!("Tuwunel binary not found — downloading {TUWUNEL_VERSION}...");

    let url = build_download_url()?;
    let zst_path = bin_path.with_extension("zst");

    if let Err(e) = download_file(&url, &zst_path) {
        write_download_failure(&marker, &e);
        return Err(e);
    }

    decompress_zstd(&zst_path, &bin_path)?;

    // Clean up the compressed archive
    drop(std::fs::remove_file(&zst_path));

    // Make the binary executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&bin_path, perms).map_err(|e| format!("Cannot set executable permission: {e}"))?;
    }

    // Clear any stale failure marker on success
    drop(std::fs::remove_file(&marker));

    log::info!("Tuwunel {TUWUNEL_VERSION} installed at {}", bin_path.display());
    Ok(())
}

/// Build the download URL for the current CPU architecture.
fn build_download_url() -> Result<String, String> {
    let arch = std::env::consts::ARCH;
    let arch_slug = match arch {
        "x86_64" => "x86_64-v2",
        "aarch64" => "aarch64",
        _ => return Err(format!("Unsupported architecture for Tuwunel: {arch}")),
    };
    let version = TUWUNEL_VERSION;
    Ok(TUWUNEL_URL_TEMPLATE.replace(VERSION_PLACEHOLDER, version).replace(ARCH_PLACEHOLDER, arch_slug))
}

/// Download a file from a URL to a local path (blocking).
fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let resp = reqwest::blocking::get(url).map_err(|e| format!("Download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download returned HTTP {}", resp.status()));
    }

    let bytes = resp.bytes().map_err(|e| format!("Failed to read download body: {e}"))?;
    std::fs::write(dest, &bytes).map_err(|e| format!("Cannot write {}: {e}", dest.display()))?;

    log::info!("Downloaded {} ({} bytes)", dest.display(), bytes.len());
    Ok(())
}

/// Decompress a `.zst` file using the system `zstd` command.
///
/// Falls back to a clear error message if `zstd` is not installed.
fn decompress_zstd(src: &Path, dest: &Path) -> Result<(), String> {
    let status = std::process::Command::new("zstd")
        .arg("-d")
        .arg(src.as_os_str())
        .arg("-o")
        .arg(dest.as_os_str())
        .arg("--force")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "zstd command not found. Install it: sudo apt install zstd (or equivalent)".to_string()
            } else {
                format!("Failed to run zstd: {e}")
            }
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("zstd decompression failed (exit code: {})", status.code().unwrap_or(-1)))
    }
}

// -- Download failure caching ------------------------------------------------

/// Marker file path for a failed download: `<binary_path>.download-failed`.
fn download_failure_marker(bin_path: &Path) -> PathBuf {
    let mut marker = bin_path.as_os_str().to_owned();
    marker.push(".download-failed");
    PathBuf::from(marker)
}

/// Check if a recent download failure was cached.
///
/// Returns `Some(reason)` if the marker exists and is younger than
/// [`DOWNLOAD_RETRY_INTERVAL`], indicating we should skip the download.
/// Returns `None` if we should attempt the download (no marker, stale
/// marker, or unreadable marker).
fn is_download_recently_failed(marker: &Path) -> Option<String> {
    let content = std::fs::read_to_string(marker).ok()?;
    let first_line = content.lines().next()?;
    let ts_secs: u64 = first_line.parse().ok()?;

    let failed_at = SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(ts_secs))?;
    let age = SystemTime::now().duration_since(failed_at).ok()?;

    if age < DOWNLOAD_RETRY_INTERVAL {
        let reason = content.lines().nth(1).unwrap_or("download previously failed");
        let hours_left = DOWNLOAD_RETRY_INTERVAL.saturating_sub(age).as_secs().checked_div(3600).unwrap_or(0);
        Some(format!("Download skipped (cached failure, retry in ~{hours_left}h): {reason}"))
    } else {
        // Marker is stale — remove it and retry
        let _r = std::fs::remove_file(marker);
        None
    }
}

/// Write a download failure marker with the current timestamp and error.
fn write_download_failure(marker: &Path, error: &str) {
    let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let content = format!("{ts}\n{error}\n");
    // Best-effort — if the directory doesn't exist, just skip
    let _r = std::fs::write(marker, content);
}
