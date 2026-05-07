//! Init-time helpers: index creation, metrics population, project hashing.
//!
//! Extracted from `lib.rs` to keep the module trait implementation focused.
//! Called during `init_state` / `load_module_data` — not on the hot path.

use super::client;
use crate::types;

/// Compute an 8-character hex hash of a path for per-project index naming.
pub(crate) fn hash_project_path(path: &str) -> String {
    use sha2::Digest as _;
    let hash = sha2::Sha256::digest(path.as_bytes());
    // Take first 4 bytes → 8 hex chars
    hex_encode_4_bytes(hash.as_slice())
}

/// Encode the first 4 bytes of a slice as an 8-character lowercase hex string.
fn hex_encode_4_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(8);
    for &b in bytes.iter().take(4) {
        use std::fmt::Write as _;
        let _r = write!(out, "{b:02x}");
    }
    out
}

/// Create per-project Meilisearch indexes if they don't already exist.
///
/// Creates `cp_{hash}_files` and `cp_{hash}_logs` indexes with appropriate
/// settings (searchable, filterable, sortable attributes).
///
/// # Errors
///
/// Returns an error if any API call fails.
pub(crate) fn ensure_indexes(port: u16, master_key: &str, project_hash: &str) -> Result<(), String> {
    let meili = client::MeiliClient::new(port, master_key)?;

    let files_uid = format!("cp_{project_hash}_files");
    let logs_uid = format!("cp_{project_hash}_logs");

    // Enable vector store (required for embedders, idempotent)
    if let Err(e) = meili.enable_vector_store() {
        log::warn!("Could not enable vector store: {e}");
    }

    // Files index
    if !meili.index_exists(&files_uid)? {
        let create_task = meili.create_index(&files_uid, "id")?;
        meili.wait_for_task(create_task)?;
        let settings_task = meili.update_settings(&files_uid, &types::files_index_settings())?;
        meili.wait_for_task(settings_task)?;
        log::info!("Created files index: {files_uid}");
    }

    // Logs index
    if !meili.index_exists(&logs_uid)? {
        let create_task = meili.create_index(&logs_uid, "id")?;
        meili.wait_for_task(create_task)?;
        let settings_task = meili.update_settings(&logs_uid, &types::logs_index_settings())?;
        meili.wait_for_task(settings_task)?;
        log::info!("Created logs index: {logs_uid}");
    }

    // Configure embedders for hybrid search (idempotent — only if not already set)
    configure_embedders(&meili, &files_uid, &logs_uid);

    Ok(())
}

/// Query Meilisearch for initial index statistics and populate metrics.
///
/// Called once during `init_state` / `load_module_data` so the Ctrl+I overlay
/// shows correct counts immediately (before the indexer has done any work).
/// Queries both basic stats (doc count) and facet distributions (extension
/// breakdown, chunk type split).
pub(crate) fn populate_initial_metrics(
    port: u16,
    master_key: &str,
    project_hash: &str,
    metrics: &std::sync::Arc<std::sync::Mutex<types::SearchMetrics>>,
) {
    let Ok(meili) = client::MeiliClient::new(port, master_key) else {
        return;
    };

    let files_uid = format!("cp_{project_hash}_files");
    let logs_uid = format!("cp_{project_hash}_logs");

    let (mut chunks, files) = if let Ok((count, _indexing)) = meili.index_stats(&files_uid) {
        let f = count.checked_div(3).unwrap_or(0).max(u64::from(count > 0));
        (count, f)
    } else {
        (0, 0)
    };

    // Also count logs (optional — just for awareness)
    if let Ok((log_count, _)) = meili.index_stats(&logs_uid) {
        chunks = chunks.saturating_add(log_count);
    }

    // Query facet distributions for extension breakdown + chunk type split
    let mut extension_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut tree_sitter_chunks: u64 = 0;
    let mut fallback_chunks: u64 = 0;

    if let Ok(facets) = meili.facet_distribution(&files_uid, &["extension", "chunk_type"]) {
        // Parse extension counts: { "extension": { "rs": 3000, "py": 200, ... } }
        if let Some(ext_map) = facets.get("extension").and_then(serde_json::Value::as_object) {
            for (ext, count) in ext_map {
                if let Some(n) = count.as_u64() {
                    let _prev = extension_counts.insert(ext.clone(), n);
                }
            }
        }

        // Parse chunk type counts: { "chunk_type": { "function": 1500, "raw": 200, ... } }
        if let Some(ct_map) = facets.get("chunk_type").and_then(serde_json::Value::as_object) {
            for (chunk_type, count) in ct_map {
                if let Some(n) = count.as_u64() {
                    if chunk_type == "raw" {
                        fallback_chunks = fallback_chunks.saturating_add(n);
                    } else {
                        tree_sitter_chunks = tree_sitter_chunks.saturating_add(n);
                    }
                }
            }
        }
    }

    // Infer OCR metrics from the disk cache + Meilisearch facets.
    // This recovers baseline OCR stats even if the persisted data was lost
    // (e.g., first reload after the persistence fix was deployed).
    // Must happen BEFORE file_ext_counts, which may move extension_counts.
    let ocr_cache_count = count_ocr_cache_files();
    let ocr_indexed_count = count_ocr_indexed_files(&extension_counts);

    // Derive file count from extension counts (more accurate than chunk/3 estimate)
    // Each file produces multiple chunks, but the extension facet counts chunks not files.
    // We keep the estimate from stats for files, but use facet data for extension ratios.
    // Convert chunk-per-extension to approximate file-per-extension using the ratio.
    let total_ext_chunks: u64 = extension_counts.values().sum();
    let file_ext_counts: std::collections::HashMap<String, u64> = if total_ext_chunks > 0 && files > 0 {
        extension_counts
            .iter()
            .map(|(ext, &chunk_count)| {
                let file_count = chunk_count
                    .saturating_mul(files)
                    .checked_div(total_ext_chunks)
                    .unwrap_or(0)
                    .max(u64::from(chunk_count > 0));
                (ext.clone(), file_count)
            })
            .collect()
    } else {
        extension_counts
    };

    if let Ok(mut m) = metrics.lock() {
        m.chunks_indexed = chunks;
        m.files_indexed = files;
        m.extension_counts = file_ext_counts;
        m.tree_sitter_chunks = tree_sitter_chunks;
        m.fallback_chunks = fallback_chunks;

        // Bootstrap OCR metrics from cache/index if persist data is zero.
        // Once real persist data exists (after a save with the fix), these
        // inferred values will be overwritten by the caller.
        if m.ocr_attempted == 0 && (ocr_cache_count > 0 || ocr_indexed_count > 0) {
            let inferred = ocr_cache_count.max(ocr_indexed_count);
            m.ocr_attempted = inferred;
            m.ocr_succeeded = inferred;
            m.ocr_cached = ocr_cache_count;
            m.ocr_enabled = true;
        }
    }
}

/// Count `.md` files in the global OCR cache directory.
///
/// Each file represents a previously successful OCR conversion.
/// Returns 0 if the cache directory doesn't exist or can't be read.
fn count_ocr_cache_files() -> u64 {
    let Some(dir) = std::env::var("HOME").ok().map(|h| std::path::PathBuf::from(h).join(".context-pilot/ocr-cache"))
    else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let count = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(std::ffi::OsStr::to_str) == Some("md"))
        .count();
    u64::try_from(count).unwrap_or(0)
}

/// Count how many indexed chunks have OCR-eligible extensions.
///
/// Checks the facet-derived extension counts for extensions that
/// require OCR processing (pdf, png, jpg, etc.).  Returns chunk
/// count (not file count) — used as a lower bound for OCR activity.
fn count_ocr_indexed_files(extension_counts: &std::collections::HashMap<String, u64>) -> u64 {
    extension_counts.iter().filter(|(ext, _)| crate::ocr::is_ocr_extension(ext)).map(|(_, count)| *count).sum()
}

// -- Embedder configuration --------------------------------------------------

/// The `huggingFace` model used for local embeddings.
///
/// BGE-small-en-v1.5: 33M params, 384 dims, ~130 MB download.
/// Good quality/speed tradeoff for code and text search.
const EMBEDDER_MODEL: &str = "BAAI/bge-small-en-v1.5";

/// Configure embedders on the files and logs indexes if not already set.
///
/// Uses the built-in `huggingFace` source which runs embeddings locally
/// via `candle` — no external service needed.
///
/// This is a fire-and-forget operation: we submit the settings update and
/// let Meilisearch generate embeddings in the background. For ~4000 chunks
/// on Apple Silicon, initial embedding takes roughly 3–5 minutes.
fn configure_embedders(meili: &client::MeiliClient, files_uid: &str, logs_uid: &str) {
    // Check if files index already has embedders configured
    let files_has_embedder =
        meili.get_embedder_settings(files_uid).ok().and_then(|v| v.as_object().map(|m| !m.is_empty())).unwrap_or(false);

    if !files_has_embedder {
        let settings = files_embedder_settings();
        match meili.update_embedder_settings(files_uid, &settings) {
            Ok(task_uid) => log::info!("Configuring embedder for {files_uid} (task {task_uid})"),
            Err(e) => log::warn!("Failed to configure embedder for {files_uid}: {e}"),
        }
    }

    // Check if logs index already has embedders configured
    let logs_has_embedder =
        meili.get_embedder_settings(logs_uid).ok().and_then(|v| v.as_object().map(|m| !m.is_empty())).unwrap_or(false);

    if !logs_has_embedder {
        let settings = logs_embedder_settings();
        match meili.update_embedder_settings(logs_uid, &settings) {
            Ok(task_uid) => log::info!("Configuring embedder for {logs_uid} (task {task_uid})"),
            Err(e) => log::warn!("Failed to configure embedder for {logs_uid}: {e}"),
        }
    }
}

/// Embedder settings for the files index.
///
/// Uses Liquid template to combine file path, chunk type/name, and content
/// into a rich embedding input that captures WHERE and WHAT the code is.
fn files_embedder_settings() -> serde_json::Value {
    serde_json::json!({
        "default": {
            "source": "huggingFace",
            "model": EMBEDDER_MODEL,
            "documentTemplate": "{{doc.file_path}} [{{doc.chunk_type}}] {{doc.chunk_name}}: {{doc.content | truncatewords: 100}}",
            "documentTemplateMaxBytes": 1024
        }
    })
}

/// Embedder settings for the logs index.
///
/// Simpler template since logs are short free-text entries.
fn logs_embedder_settings() -> serde_json::Value {
    serde_json::json!({
        "default": {
            "source": "huggingFace",
            "model": EMBEDDER_MODEL,
            "documentTemplate": "[{{doc.importance}}] {{doc.content}}"
        }
    })
}
