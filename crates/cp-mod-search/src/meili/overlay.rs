//! Ctrl+I overlay data provider.
//!
//! Fetches live Meilisearch stats and builds [`SearchOverlayInfo`]
//! for the main binary's overlay renderer.  Stats are cached for
//! 2 seconds to avoid hammering the local server from the render loop.

use cp_base::state::runtime::State;

use super::client;
use crate::types::{MeiliLiveStats, SearchOverlayInfo, SearchState};

/// Read overlay information from the search module's state.
///
/// Returns `None` if the search module hasn't been initialized.
/// Used by the main binary's Ctrl+I overlay renderer.
///
/// Fetches live stats from Meilisearch at most once every 2 seconds
/// (cached in `SearchMetrics.live_stats`). The HTTP call is made
/// outside any lock to avoid blocking.
#[must_use]
pub(crate) fn overlay_info(state: &State) -> Option<SearchOverlayInfo> {
    let ss = state.get_ext::<SearchState>()?;

    // Refresh live stats from Meilisearch (max once per 2s, no lock held during HTTP)
    refresh_live_stats(ss);

    let metrics = ss.metrics.lock().ok()?;

    // Sort extensions by count descending, take top 8.
    let mut ext_vec: Vec<(String, u64)> = metrics.extension_counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    ext_vec.sort_by_key(|e| std::cmp::Reverse(e.1));
    ext_vec.truncate(8);

    // Sort recompute counts descending, take top 8
    let mut top_recomputed: Vec<(String, u64)> =
        metrics.recompute_counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    top_recomputed.sort_by_key(|e| std::cmp::Reverse(e.1));
    top_recomputed.truncate(8);

    // Sort last_sent_ms descending (most recent first), take top 8
    let mut recently_sent: Vec<(String, u64)> = metrics.last_sent_ms.iter().map(|(k, v)| (k.clone(), *v)).collect();
    recently_sent.sort_by_key(|e| std::cmp::Reverse(e.1));
    recently_sent.truncate(8);

    // Extract live stats (or defaults)
    let live = metrics.live_stats.clone().unwrap_or_default();

    Some(SearchOverlayInfo {
        port: ss.persist.port,
        chunks_indexed: metrics.chunks_indexed,
        files_indexed: metrics.files_indexed,
        queue_depth: metrics.queue_depth,
        error_count: metrics.error_count,
        last_activity_ms: metrics.last_activity_ms,
        index_ready: metrics.scan_complete,
        top_extensions: ext_vec,
        tree_sitter_chunks: metrics.tree_sitter_chunks,
        fallback_chunks: metrics.fallback_chunks,
        ocr_attempted: metrics.ocr_attempted,
        ocr_succeeded: metrics.ocr_succeeded,
        ocr_failed: metrics.ocr_failed,
        ocr_cached: metrics.ocr_cached,
        ocr_available: metrics.ocr_enabled,
        database_size_bytes: live.database_size_bytes,
        used_database_size_bytes: live.used_database_size_bytes,
        files_embedding_count: live.files_embedding_count,
        files_is_indexing: live.files_is_indexing,
        logs_doc_count: live.logs_doc_count,
        embedding_model: live.embedding_model.clone(),
        meili_version: live.version,
        avg_document_size: live.avg_document_size,
        raw_document_db_size: live.raw_document_db_size,
        files_embedded_doc_count: live.files_embedded_doc_count,
        files_total_doc_count: live.files_total_doc_count,
        last_update: live.last_update,
        recent_tasks: live
            .recent_tasks
            .iter()
            .map(|t| crate::types::MeiliTaskInfo {
                uid: t.uid,
                task_type: shorten_task_type(&t.task_type),
                status: t.status.clone(),
                duration: humanize_duration(&t.duration),
            })
            .collect(),
        top_recomputed,
        recently_sent,
    })
}

/// Current time in milliseconds since Unix epoch.
fn current_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Refresh cached live stats from Meilisearch if stale (>2s old).
///
/// Makes HTTP calls (`/stats` + `/settings/embedders`) outside any lock.
fn refresh_live_stats(ss: &SearchState) {
    if ss.persist.port == 0 {
        return;
    }

    let now_ms = current_ms();

    // Check if cached stats are fresh enough (lock held briefly)
    let is_stale = ss
        .metrics
        .lock()
        .ok()
        .is_none_or(|m| m.live_stats.as_ref().is_none_or(|s| now_ms.saturating_sub(s.fetched_at_ms) > 2000));

    if !is_stale {
        return;
    }

    // Fetch live stats — no lock held during network I/O
    let Ok(meili) = client::MeiliClient::new(ss.persist.port, &ss.persist.master_key) else {
        return;
    };
    let Ok(stats) = meili.global_stats() else {
        return;
    };

    let files_uid = format!("cp_{}_files", ss.persist.project_hash);
    let logs_uid = format!("cp_{}_logs", ss.persist.project_hash);

    // Read embedding model name from embedder settings (cached alongside stats)
    let model = meili
        .get_embedder_settings(&files_uid)
        .ok()
        .and_then(|v| v.get("default")?.get("model")?.as_str().map(String::from))
        .unwrap_or_default();

    // Fetch server version (cheap GET, never changes but simpler to always fetch)
    let version = meili.version().unwrap_or_default();

    // Fetch recent tasks filtered to this project's indexes
    let tasks_json =
        meili.recent_tasks(5, &[&files_uid, &logs_uid]).unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));

    // -- Parse stats into MeiliLiveStats (inlined to avoid too_many_arguments) --

    let db_size = stats.get("databaseSize").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let db_used = stats.get("usedDatabaseSize").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let last_update = stats.get("lastUpdate").and_then(serde_json::Value::as_str).unwrap_or("").to_string();

    let indexes = stats.get("indexes");

    let files_stats = indexes.and_then(|i| i.get(&files_uid));
    let emb_count =
        files_stats.and_then(|f| f.get("numberOfEmbeddings")).and_then(serde_json::Value::as_u64).unwrap_or(0);
    let is_indexing =
        files_stats.and_then(|f| f.get("isIndexing")).and_then(serde_json::Value::as_bool).unwrap_or(false);
    let avg_doc_size =
        files_stats.and_then(|f| f.get("avgDocumentSize")).and_then(serde_json::Value::as_u64).unwrap_or(0);
    let raw_doc_db =
        files_stats.and_then(|f| f.get("rawDocumentDbSize")).and_then(serde_json::Value::as_u64).unwrap_or(0);
    let embedded_count =
        files_stats.and_then(|f| f.get("numberOfEmbeddedDocuments")).and_then(serde_json::Value::as_u64).unwrap_or(0);
    let total_count =
        files_stats.and_then(|f| f.get("numberOfDocuments")).and_then(serde_json::Value::as_u64).unwrap_or(0);

    let logs_stats = indexes.and_then(|i| i.get(&logs_uid));
    let logs_count =
        logs_stats.and_then(|l| l.get("numberOfDocuments")).and_then(serde_json::Value::as_u64).unwrap_or(0);

    // Parse recent tasks
    let recent_tasks = tasks_json
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    Some(crate::types::MeiliTask {
                        uid: t.get("uid")?.as_u64()?,
                        task_type: t.get("type")?.as_str()?.to_string(),
                        status: t.get("status")?.as_str()?.to_string(),
                        duration: t.get("duration").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let live = MeiliLiveStats {
        database_size_bytes: db_size,
        used_database_size_bytes: db_used,
        files_embedding_count: emb_count,
        files_is_indexing: is_indexing,
        logs_doc_count: logs_count,
        embedding_model: model,
        fetched_at_ms: current_ms(),
        version,
        avg_document_size: avg_doc_size,
        raw_document_db_size: raw_doc_db,
        files_embedded_doc_count: embedded_count,
        files_total_doc_count: total_count,
        last_update,
        recent_tasks,
    };

    // Write to cache (lock held briefly)
    if let Ok(mut m) = ss.metrics.lock() {
        m.live_stats = Some(live);
    }
}

/// Shorten Meilisearch task type names for compact display.
fn shorten_task_type(task_type: &str) -> String {
    match task_type {
        "documentAdditionOrUpdate" => "docAdd".to_string(),
        "documentDeletion" => "docDel".to_string(),
        "settingsUpdate" => "settings".to_string(),
        "indexCreation" => "create".to_string(),
        "indexUpdate" => "update".to_string(),
        "indexDeletion" => "delete".to_string(),
        "indexSwap" => "swap".to_string(),
        "taskCancelation" => "cancel".to_string(),
        "taskDeletion" => "taskDel".to_string(),
        "dumpCreation" => "dump".to_string(),
        "snapshotCreation" => "snapshot".to_string(),
        other => other.to_string(),
    }
}

/// Convert an ISO 8601 duration string (e.g. "PT0.254092S") to a short
/// human-readable form (e.g. "0.25s"). Returns "—" for empty strings.
fn humanize_duration(iso: &str) -> String {
    if iso.is_empty() {
        return "\u{2014}".to_string(); // em-dash
    }

    // Strip "PT" prefix and "S" suffix: "PT0.254092S" → "0.254092"
    let stripped = iso.strip_prefix("PT").unwrap_or(iso);
    let stripped = stripped.strip_suffix('S').unwrap_or(stripped);

    // Truncate to 2 decimal places for display
    if let Some((whole, frac)) = stripped.split_once('.') {
        let short_frac = frac.get(..2).unwrap_or(frac);
        format!("{whole}.{short_frac}s")
    } else {
        format!("{stripped}s")
    }
}
