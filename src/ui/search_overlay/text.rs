//! Plain-text export of the Meilisearch indexing status overlay.
//!
//! Produces a clipboard-friendly text version of the same data
//! shown in the Ctrl+I TUI overlay. Used by the `CopyIndexOverlay`
//! action (Ctrl+C while the overlay is open).

use super::{format_ago, format_bytes};
use crate::state::State;

/// Build the overlay content as plain text for clipboard copying.
///
/// Produces a clean, terminal-agnostic text version of the overlay
/// that can be pasted into chat messages, issue reports, etc.
pub(crate) fn build_overlay_text(state: &State) -> String {
    use std::fmt::Write as _;

    let Some(info) = cp_mod_search::overlay_info(state) else {
        return "Search module not initialized.\n".to_string();
    };

    let mut out = String::with_capacity(512);

    // Server
    let version = if info.meili_version.is_empty() { String::new() } else { format!("  v{}", info.meili_version) };
    writeln!(
        out,
        "Indexing Status\n\nServer  http://127.0.0.1:{}  {}{version}\n",
        info.port,
        if info.port > 0 { "online" } else { "offline" },
    )
    .unwrap_or(());

    // Process stats
    if info.meili_memory_bytes > 0 || info.meili_cpu_pct > 0.0 {
        writeln!(out, "Process CPU {:.1}%    RAM {}\n", info.meili_cpu_pct, format_bytes(info.meili_memory_bytes),)
            .unwrap_or(());
    }

    // Database
    if info.database_size_bytes > 0 {
        writeln!(
            out,
            "── Database ──\nDisk  {} / {}    Docs  {}",
            format_bytes(info.used_database_size_bytes),
            format_bytes(info.database_size_bytes),
            format_bytes(info.raw_document_db_size),
        )
        .unwrap_or(());
        if info.avg_document_size > 0 {
            writeln!(out, "Avg chunk  {}", format_bytes(info.avg_document_size)).unwrap_or(());
        }
        out.push('\n');
    }

    // Core stats
    writeln!(
        out,
        "Files  {}    Chunks  {}\nQueue  {} pending    Errors  {}\nStatus {}    Last  {}",
        info.files_indexed,
        info.chunks_indexed,
        info.queue_depth,
        info.error_count,
        if info.index_ready { "Ready" } else { "Scanning" },
        if info.last_activity_ms > 0 { format_ago(info.last_activity_ms) } else { "never".to_string() },
    )
    .unwrap_or(());

    // Extensions
    if !info.top_extensions.is_empty() {
        out.push_str("\n── Extensions ──\n");
        let total: u64 = info.top_extensions.iter().map(|e| e.1).sum();
        for (ext, count) in &info.top_extensions {
            let pct = if total > 0 { count.saturating_mul(100).checked_div(total).unwrap_or(0) } else { 0 };
            writeln!(out, "  {ext:<6} {count:>4}  {pct}%").unwrap_or(());
        }
    }

    // Splitter
    let total_chunks = info.tree_sitter_chunks.saturating_add(info.fallback_chunks);
    if total_chunks > 0 {
        let ts_pct = info.tree_sitter_chunks.saturating_mul(100).checked_div(total_chunks).unwrap_or(0);
        let fb_pct = 100_u64.saturating_sub(ts_pct);
        writeln!(
            out,
            "\n── Splitter ──\nTree-sitter  {} chunks ({ts_pct}%)\nFallback     {} chunks ({fb_pct}%)",
            info.tree_sitter_chunks, info.fallback_chunks,
        )
        .unwrap_or(());
    }

    // Embeddings
    if !info.embedding_model.is_empty() || info.files_embedding_count > 0 {
        out.push_str("\n── Embeddings ──\n");
        if !info.embedding_model.is_empty() {
            writeln!(out, "Model   {}", info.embedding_model).unwrap_or(());
        }
        let status = if info.files_is_indexing { "generating" } else { "ready" };
        writeln!(out, "Vectors {}  {status}", info.files_embedding_count).unwrap_or(());
        if info.files_total_doc_count > 0 {
            let pct =
                info.files_embedded_doc_count.saturating_mul(100).checked_div(info.files_total_doc_count).unwrap_or(0);
            writeln!(out, "Coverage {}/{}  ({pct}%)", info.files_embedded_doc_count, info.files_total_doc_count)
                .unwrap_or(());
        }
        if info.logs_doc_count > 0 {
            writeln!(out, "Logs    {} documents", info.logs_doc_count).unwrap_or(());
        }
    }

    // OCR
    if info.ocr_available || info.ocr_attempted > 0 {
        out.push_str("\n── OCR Pipeline ──\n");
        if info.ocr_attempted > 0 {
            writeln!(
                out,
                "Attempted  {}   Succeeded  {}   Cached  {}",
                info.ocr_attempted, info.ocr_succeeded, info.ocr_cached,
            )
            .unwrap_or(());
            if info.ocr_failed > 0 {
                writeln!(out, "Failed     {}", info.ocr_failed).unwrap_or(());
            }
        } else {
            out.push_str("Enabled — no OCR files found yet\n");
        }
    }

    // Recent Tasks
    if !info.recent_tasks.is_empty() {
        out.push_str("\n── Recent Tasks ──\n");
        for task in &info.recent_tasks {
            writeln!(out, "  #{:<6} {:<10} {:<10} {}", task.uid, task.task_type, task.status, task.duration)
                .unwrap_or(());
        }
    }

    // Top Recomputed
    if !info.top_recomputed.is_empty() {
        out.push_str("\n── Top Recomputed ──\n");
        for (path, count) in &info.top_recomputed {
            writeln!(out, "  {count:>4}×  {path}").unwrap_or(());
        }
    }

    // Recently Sent
    if !info.recently_sent.is_empty() {
        out.push_str("\n── Recently Sent ──\n");
        for (path, ts_ms) in &info.recently_sent {
            let ago = if *ts_ms > 0 { format_ago(*ts_ms) } else { "?".to_string() };
            writeln!(out, "  {ago:>8}  {path}").unwrap_or(());
        }
    }

    out
}
