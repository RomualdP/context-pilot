//! Background file indexer thread.
//!
//! Receives file-system events via an [`mpsc`] channel, reads and
//! chunks the files using the [`SplitterChain`], and indexes the
//! resulting documents into Meilisearch.
//!
//! Also performs the initial full-project scan on first boot.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::client::MeiliClient;
use crate::config;
use crate::splitter::SplitterChain;
use crate::types::{IndexerCmd, SearchMetrics};

/// Duration to wait after the first event before processing a batch.
const DEBOUNCE_MS: u64 = 200;

/// Start the background indexer and file watcher.
///
/// Returns the command sender and watcher handle.  The indexer thread
/// runs until the sender is dropped.
///
/// # Errors
///
/// Returns an error if the Meilisearch client or file watcher cannot
/// be created.
pub(crate) fn start(
    port: u16,
    master_key: &str,
    project_hash: &str,
    project_root: PathBuf,
    metrics: std::sync::Arc<std::sync::Mutex<SearchMetrics>>,
) -> Result<(mpsc::Sender<IndexerCmd>, RecommendedWatcher), String> {
    let (tx, rx) = mpsc::channel::<IndexerCmd>();

    // Clone sender for the watcher callback
    let watcher_tx = tx.clone();

    // Set up file watcher
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                for path in &event.paths {
                    let cmd = match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => IndexerCmd::IndexFile(path.clone()),
                        EventKind::Remove(_) => IndexerCmd::DeleteFile(path.clone()),
                        _ => continue,
                    };
                    let _r = watcher_tx.send(cmd);
                }
            }
        },
        notify::Config::default(),
    )
    .map_err(|e| format!("Cannot create file watcher: {e}"))?;

    watcher.watch(&project_root, RecursiveMode::Recursive).map_err(|e| format!("Cannot watch project root: {e}"))?;

    // Spawn initial scan on a helper thread (queues IndexFile commands)
    let scan_tx = tx.clone();
    let scan_root = project_root.clone();
    let _handle = std::thread::Builder::new()
        .name("search-scan".into())
        .spawn(move || {
            scan_directory(&scan_tx, &scan_root);
        })
        .map_err(|e| format!("Cannot spawn scan thread: {e}"))?;

    // Spawn the indexer thread
    let idx_key = master_key.to_string();
    let idx_hash = project_hash.to_string();
    let _handle = std::thread::Builder::new()
        .name("search-indexer".into())
        .spawn(move || {
            indexer_loop(rx, port, &idx_key, &idx_hash, &project_root, metrics);
        })
        .map_err(|e| format!("Cannot spawn indexer thread: {e}"))?;

    Ok((tx, watcher))
}

// -- Indexer loop ------------------------------------------------------------

/// Main loop of the background indexer thread.
///
/// Blocks on the receiver, debounces incoming events for 200 ms,
/// deduplicates them, and processes each command.
fn indexer_loop(
    rx: mpsc::Receiver<IndexerCmd>,
    port: u16,
    master_key: &str,
    project_hash: &str,
    project_root: &Path,
    metrics: std::sync::Arc<std::sync::Mutex<SearchMetrics>>,
) {
    let client = match MeiliClient::new(port, master_key) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Indexer: cannot create Meilisearch client: {e}");
            return;
        }
    };

    let files_uid = format!("cp_{project_hash}_files");
    let splitter = SplitterChain::new();

    loop {
        // Block until first command arrives
        let first = match rx.recv() {
            Ok(cmd) => cmd,
            Err(_) => break, // sender dropped
        };

        let mut batch = vec![first];

        // Debounce: collect more events for DEBOUNCE_MS
        let deadline = Instant::now() + Duration::from_millis(DEBOUNCE_MS);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(cmd) => {
                    batch.push(cmd);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }

        // Deduplicate: keep only the latest operation per path
        let unique = deduplicate(batch);

        for cmd in unique {
            match cmd {
                IndexerCmd::IndexFile(path) => {
                    index_one_file(&client, &files_uid, &path, project_root, &splitter, &metrics);
                }
                IndexerCmd::DeleteFile(path) => {
                    delete_one_file(&client, &files_uid, &path, project_root, &metrics);
                }
            }
        }
    }
}

// -- Deduplication -----------------------------------------------------------

/// Keep only the latest command per path.
///
/// If the same path appears multiple times (e.g., rapid saves),
/// only the last command (Index or Delete) is kept.
fn deduplicate(batch: Vec<IndexerCmd>) -> Vec<IndexerCmd> {
    let mut latest: HashMap<PathBuf, IndexerCmd> = HashMap::new();

    for cmd in batch {
        match &cmd {
            IndexerCmd::IndexFile(p) | IndexerCmd::DeleteFile(p) => {
                let _prev = latest.insert(p.clone(), cmd);
            }
        }
    }

    latest.into_values().collect()
}

// -- File indexing -----------------------------------------------------------

/// Index a single file: read → filter → split → upload.
fn index_one_file(
    client: &MeiliClient,
    files_uid: &str,
    abs_path: &Path,
    project_root: &Path,
    splitter: &SplitterChain,
    _metrics: &std::sync::Arc<std::sync::Mutex<SearchMetrics>>,
) {
    // Skip symlinks
    if abs_path.is_symlink() {
        return;
    }

    // Relative path for storage
    let rel_path = abs_path.strip_prefix(project_root).unwrap_or(abs_path);
    let rel_str = rel_path.to_string_lossy();

    // Check path exclusions (directory components)
    for component in rel_path.components() {
        if let std::path::Component::Normal(name) = component {
            let name_str = name.to_str().unwrap_or("");
            if config::is_excluded_dir(name_str) {
                return;
            }
        }
    }

    // Check extension allowlist
    let ext = rel_path.extension().and_then(std::ffi::OsStr::to_str).unwrap_or("");
    if !config::is_allowed_extension(ext) {
        return;
    }

    // Check excluded file patterns
    let filename = rel_path.file_name().and_then(std::ffi::OsStr::to_str).unwrap_or("");
    if config::is_excluded_file(filename) {
        return;
    }

    // Check file size
    let meta = match std::fs::metadata(abs_path) {
        Ok(m) => m,
        Err(_) => return,
    };
    if meta.len() > config::MAX_FILE_SIZE {
        return;
    }

    // Read content (skip binary files that fail UTF-8)
    let content = match std::fs::read_to_string(abs_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Delete existing chunks for this path (delete → re-insert strategy)
    let escaped = rel_str.replace('\'', "\\'");
    let filter = format!("file_path = '{escaped}'");
    if let Ok(task) = client.delete_documents_by_filter(files_uid, &filter) {
        let _r = client.wait_for_task(task);
    }

    // Split into chunks
    let chunks = splitter.split(&content, rel_path);
    if chunks.is_empty() {
        return;
    }

    // Build Meilisearch documents
    let last_modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);

    let docs: Vec<serde_json::Value> = chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            serde_json::json!({
                "id": format!("{rel_str}:{i}"),
                "file_path": rel_str,
                "content": chunk.content,
                "extension": ext,
                "chunk_type": chunk.kind,
                "chunk_name": chunk.name,
                "line_start": chunk.line_start,
                "line_end": chunk.line_end,
                "char_start": chunk.char_start,
                "char_end": chunk.char_end,
                "last_modified_ms": last_modified_ms,
            })
        })
        .collect();

    // Send to Meilisearch
    if let Ok(task) = client.add_documents(files_uid, &serde_json::Value::Array(docs)) {
        let _r = client.wait_for_task(task);
    }
}

/// Delete all indexed chunks for a single file.
fn delete_one_file(
    client: &MeiliClient,
    files_uid: &str,
    abs_path: &Path,
    project_root: &Path,
    _metrics: &std::sync::Arc<std::sync::Mutex<SearchMetrics>>,
) {
    let rel_path = abs_path.strip_prefix(project_root).unwrap_or(abs_path);
    let rel_str = rel_path.to_string_lossy();
    let escaped = rel_str.replace('\'', "\\'");
    let filter = format!("file_path = '{escaped}'");

    if let Ok(task) = client.delete_documents_by_filter(files_uid, &filter) {
        let _r = client.wait_for_task(task);
    }
}

// -- Directory scan ----------------------------------------------------------

/// Recursively scan a directory and queue eligible files for indexing.
///
/// Skips symlinks, excluded directories, and sends `IndexFile` for
/// every regular file encountered.  Filtering (extension, size) is
/// done by the indexer thread when it processes each command.
fn scan_directory(tx: &mpsc::Sender<IndexerCmd>, dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip symlinks
        if path.is_symlink() {
            continue;
        }

        if path.is_dir() {
            let name = entry.file_name();
            let name_str = name.to_str().unwrap_or("");
            if !config::is_excluded_dir(name_str) {
                scan_directory(tx, &path);
            }
        } else if path.is_file() {
            let _r = tx.send(IndexerCmd::IndexFile(path));
        }
    }
}
