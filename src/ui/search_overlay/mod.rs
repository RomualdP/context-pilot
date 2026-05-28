//! Ctrl+I Meilisearch indexing status overlay.
//!
//! Renders a floating, centered info box showing the Meilisearch server
//! status, indexing metrics, extension breakdown, splitter stats,
//! and OCR pipeline status.

/// Plain-text export of the indexing overlay for clipboard copy.
pub(crate) mod text;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::{Rect, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::state::State;
use crate::ui::theme;

/// Overlay width in terminal cells (two-column layout).
const OVERLAY_WIDTH: u16 = 120;

/// Render the Meilisearch indexing status overlay.
///
/// Displays server status, index metrics, extension breakdown, splitter
/// stats, and OCR pipeline info in a centered, bordered box.
pub(crate) fn render_index_overlay(frame: &mut Frame<'_>, state: &State, area: Rect) {
    let (left_lines, right_lines) = build_overlay_columns(state);

    let left_len = u16::try_from(left_lines.len().saturating_add(2)).unwrap_or(30);
    let right_len = u16::try_from(right_lines.len().saturating_add(2)).unwrap_or(30);
    let height = left_len.max(right_len).min(area.height);
    let popup = centered_rect(OVERLAY_WIDTH, height, area);

    // Show "✓ Copied!" flash in title for 1.5 seconds after copy
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
    let now_u64 = u64::try_from(now_ms).unwrap_or(u64::MAX);
    let flash_active =
        state.flags.overlays.copied_flash_ms > 0 && now_u64.saturating_sub(state.flags.overlays.copied_flash_ms) < 1500;
    let title = if flash_active { " ✓ Copied! " } else { " Indexing Status " };
    let footer = " Ctrl+C copy · Ctrl+I or Esc to dismiss ";

    let block = Block::default()
        .title(title)
        .title_bottom(footer)
        .borders(Borders::ALL)
        .style(Style::default().bg(theme::bg_base()).fg(theme::text()));

    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);

    // Split inner area into left | separator | right
    let columns = Layout::horizontal([Constraint::Fill(1), Constraint::Length(1), Constraint::Fill(1)]).split(inner);

    let left_para = Paragraph::new(left_lines);
    let right_para = Paragraph::new(right_lines);

    // Vertical separator
    let sep_height = usize::from(inner.height);
    let sep_lines: Vec<Line<'_>> =
        std::iter::repeat_with(|| Line::from(Span::styled("│", Style::default().fg(theme::text_muted()))))
            .take(sep_height)
            .collect();
    let sep_para = Paragraph::new(sep_lines);

    let (Some(&left_col), Some(&sep_col), Some(&right_col)) = (columns.first(), columns.get(1), columns.get(2)) else {
        debug_assert!(false, "overlay column layout must have 3 chunks");
        return;
    };

    frame.render_widget(left_para, left_col);
    frame.render_widget(sep_para, sep_col);
    frame.render_widget(right_para, right_col);
}

/// Build overlay content as two columns: (left, right).
///
/// Left column: server, database, core stats, extensions.
/// Right column: splitter, embeddings, OCR, recent tasks, recomputed, recently sent.
fn build_overlay_columns(state: &State) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let Some(info) = cp_mod_search::overlay_info(state) else {
        let fallback = vec![Line::from(""), Line::from("  Search module not initialized.")];
        return (fallback, Vec::new());
    };

    let left = build_left_column(&info);
    let right = build_right_column(&info);
    (left, right)
}

/// Build the left column: server, database, core stats, extensions.
fn build_left_column(info: &cp_mod_search::types::SearchOverlayInfo) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(24);

    // ── Server ──
    let server_url = format!("http://127.0.0.1:{}", info.port);
    let (status_label, status_color) =
        if info.port > 0 { ("● online", theme::success()) } else { ("○ offline", theme::error()) };

    lines.push(Line::from(""));
    let version_label =
        if info.meili_version.is_empty() { String::new() } else { format!("  v{}", info.meili_version) };
    lines.push(Line::from(vec![
        Span::raw("  Server  "),
        Span::styled(server_url, Style::default().fg(theme::text())),
        Span::raw("  "),
        Span::styled(status_label, Style::default().fg(status_color)),
        Span::styled(version_label, Style::default().fg(theme::text_muted())),
    ]));

    // ── Process ──
    if info.meili_memory_bytes > 0 || info.meili_cpu_pct > 0.0 {
        let cpu_color = if info.meili_cpu_pct < 25.0 {
            theme::success()
        } else if info.meili_cpu_pct < 50.0 {
            theme::warning()
        } else {
            theme::error()
        };
        lines.push(Line::from(vec![
            Span::raw("  Process "),
            Span::styled(format!("CPU {:.1}%", info.meili_cpu_pct), Style::default().fg(cpu_color)),
            Span::raw("    "),
            Span::styled(format!("RAM {}", format_bytes(info.meili_memory_bytes)), Style::default().fg(theme::text())),
        ]));
    }

    // ── Database ──
    if info.database_size_bytes > 0 {
        lines.push(Line::from(""));
        lines.push(section_header("Database"));
        lines.push(Line::from(vec![
            Span::raw("  Disk  "),
            Span::styled(format_bytes(info.used_database_size_bytes), Style::default().fg(theme::text())),
            Span::styled(" / ", Style::default().fg(theme::text_muted())),
            Span::styled(format_bytes(info.database_size_bytes), Style::default().fg(theme::text_muted())),
            Span::raw("    "),
            Span::styled("Docs  ", Style::default().fg(theme::text_muted())),
            Span::styled(format_bytes(info.raw_document_db_size), Style::default().fg(theme::text())),
        ]));
        if info.avg_document_size > 0 {
            lines.push(Line::from(vec![
                Span::raw("  Avg chunk  "),
                Span::styled(format_bytes(info.avg_document_size), Style::default().fg(theme::text())),
            ]));
        }
    }

    // ── Core Stats ──
    lines.push(Line::from(""));
    lines.push(section_header("Index"));
    lines.push(Line::from(format!("  Files  {:<10} Chunks  {}", info.files_indexed, info.chunks_indexed)));
    lines.push(Line::from(format!(
        "  Queue  {:<10} Errors  {}",
        format!("{} pending", info.queue_depth),
        info.error_count,
    )));
    let last = if info.last_activity_ms > 0 { format_ago(info.last_activity_ms) } else { "never".to_string() };
    let ready = if info.index_ready { "Ready" } else { "Scanning…" };
    lines.push(Line::from(format!("  Status {ready:<10} Last    {last}")));

    // ── Extensions ──
    if !info.top_extensions.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_header("Extensions"));

        let max_count = info.top_extensions.first().map_or(1, |e| e.1.max(1));
        let total_files: u64 = info.top_extensions.iter().map(|e| e.1).sum();
        let bar_max_width: u64 = 28;

        for (ext, count) in &info.top_extensions {
            let bar_len = count.saturating_mul(bar_max_width).checked_div(max_count).unwrap_or(0);
            let bar_usize = usize::try_from(bar_len).unwrap_or(0).max(1);
            let fill = "█".repeat(bar_usize);
            let pct = if total_files > 0 { count.saturating_mul(100).checked_div(total_files).unwrap_or(0) } else { 0 };
            lines.push(Line::from(vec![
                Span::raw(format!("  {ext:<6} {count:>4}  ")),
                Span::styled(fill, Style::default().fg(theme::accent())),
                Span::styled(format!("  {pct}%"), Style::default().fg(theme::text_muted())),
            ]));
        }
    }

    lines
}

/// Build the right column: splitter, embeddings, OCR, tasks, recomputed, recently sent.
fn build_right_column(info: &cp_mod_search::types::SearchOverlayInfo) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(32);

    // ── Splitter ──
    let total_chunks = info.tree_sitter_chunks.saturating_add(info.fallback_chunks);
    if total_chunks > 0 {
        lines.push(Line::from(""));
        lines.push(section_header("Splitter"));

        let ts_pct = info.tree_sitter_chunks.saturating_mul(100).checked_div(total_chunks).unwrap_or(0);
        let fb_pct = 100_u64.saturating_sub(ts_pct);

        lines.push(Line::from(vec![
            Span::raw("  Tree-sitter  "),
            Span::styled(format!("{} chunks", info.tree_sitter_chunks), Style::default().fg(theme::success())),
            Span::styled(format!("  ({ts_pct}%)"), Style::default().fg(theme::text_muted())),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  Fallback     "),
            Span::styled(format!("{} chunks", info.fallback_chunks), Style::default().fg(theme::warning())),
            Span::styled(format!("  ({fb_pct}%)"), Style::default().fg(theme::text_muted())),
        ]));
    }

    // ── Embeddings ──
    if !info.embedding_model.is_empty() || info.files_embedding_count > 0 {
        lines.push(Line::from(""));
        lines.push(section_header("Embeddings"));

        if !info.embedding_model.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("  Model   "),
                Span::styled(info.embedding_model.clone(), Style::default().fg(theme::text())),
            ]));
        }

        let emb_status =
            if info.files_is_indexing { ("● generating", theme::warning()) } else { ("✓ ready", theme::success()) };
        lines.push(Line::from(vec![
            Span::raw(format!("  Vectors {:>4}  ", info.files_embedding_count)),
            Span::styled(emb_status.0, Style::default().fg(emb_status.1)),
        ]));

        if info.files_total_doc_count > 0 {
            let coverage_pct =
                info.files_embedded_doc_count.saturating_mul(100).checked_div(info.files_total_doc_count).unwrap_or(0);
            let cov_color = if coverage_pct >= 100 { theme::success() } else { theme::warning() };
            lines.push(Line::from(vec![
                Span::raw("  Coverage "),
                Span::styled(
                    format!("{}/{}", info.files_embedded_doc_count, info.files_total_doc_count),
                    Style::default().fg(cov_color),
                ),
                Span::styled(format!("  ({coverage_pct}%)"), Style::default().fg(theme::text_muted())),
            ]));
        }

        if info.logs_doc_count > 0 {
            lines.push(Line::from(format!("  Logs    {} documents", info.logs_doc_count)));
        }
    }

    // ── OCR Pipeline ──
    if info.ocr_available || info.ocr_attempted > 0 {
        lines.push(Line::from(""));
        lines.push(section_header("OCR Pipeline"));

        if info.ocr_attempted > 0 {
            lines.push(Line::from(format!(
                "  Attempted  {}   Succeeded  {}   Cached  {}",
                info.ocr_attempted, info.ocr_succeeded, info.ocr_cached,
            )));
            if info.ocr_failed > 0 {
                lines.push(Line::from(vec![
                    Span::raw("  Failed     "),
                    Span::styled(format!("{}", info.ocr_failed), Style::default().fg(theme::error())),
                ]));
            }
        } else {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("Enabled", Style::default().fg(theme::success())),
                Span::styled(" — no OCR files found yet", Style::default().fg(theme::text_muted())),
            ]));
        }
    }

    // ── Recent Tasks ──
    if !info.recent_tasks.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_header("Recent Tasks"));
        for task in &info.recent_tasks {
            let task_color = match task.status.as_str() {
                "succeeded" => theme::success(),
                "failed" => theme::error(),
                "processing" => theme::warning(),
                _ => theme::text_muted(),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  #{:<6}", task.uid), Style::default().fg(theme::text_muted())),
                Span::raw(format!("{:<10} ", task.task_type)),
                Span::styled(format!("{:<10} ", task.status), Style::default().fg(task_color)),
                Span::styled(task.duration.clone(), Style::default().fg(theme::text_muted())),
            ]));
        }
    }

    // ── Top Recomputed ──
    if !info.top_recomputed.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_header("Top Recomputed"));
        for (path, count) in &info.top_recomputed {
            let short = truncate_path(path, 46);
            lines.push(Line::from(vec![
                Span::styled(format!("  {count:>4}×  "), Style::default().fg(theme::warning())),
                Span::styled(short, Style::default().fg(theme::text())),
            ]));
        }
    }

    // ── Recently Sent ──
    if !info.recently_sent.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_header("Recently Sent"));
        for (path, ts_ms) in &info.recently_sent {
            let short = truncate_path(path, 42);
            let ago = if *ts_ms > 0 { format_ago(*ts_ms) } else { "?".to_string() };
            lines.push(Line::from(vec![
                Span::styled(format!("  {ago:>8}  "), Style::default().fg(theme::text_muted())),
                Span::styled(short, Style::default().fg(theme::text())),
            ]));
        }
    }

    lines
}

/// Render a section header line with dashes.
fn section_header(title: &str) -> Line<'static> {
    let dashes = "─".repeat(48_usize.saturating_sub(title.len()).saturating_sub(4));
    Line::from(vec![
        Span::styled(format!("  ── {title} "), Style::default().fg(theme::accent())),
        Span::styled(dashes, Style::default().fg(theme::text_muted())),
    ])
}

/// Compute a centered rectangle within the given area.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let eff_w = width.min(area.width);
    let eff_h = height.min(area.height);
    let x_off = area.width.saturating_sub(eff_w).checked_div(2).unwrap_or(0);
    let y_off = area.height.saturating_sub(eff_h).checked_div(2).unwrap_or(0);
    Rect::new(area.x.saturating_add(x_off), area.y.saturating_add(y_off), eff_w, eff_h)
}

/// Format a millisecond timestamp as a relative "X ago" string.
pub(crate) fn format_ago(ms_then: u64) -> String {
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
    let now_u64 = u64::try_from(now_ms).unwrap_or(u64::MAX);
    let diff_sec = now_u64.saturating_sub(ms_then).checked_div(1000).unwrap_or(0);
    if diff_sec < 60 {
        format!("{diff_sec}s ago")
    } else if diff_sec < 3600 {
        format!("{}m ago", diff_sec.checked_div(60).unwrap_or(0))
    } else {
        format!("{}h ago", diff_sec.checked_div(3600).unwrap_or(0))
    }
}

/// Truncate a file path to fit within `max_len` characters.
///
/// If the path is longer, keeps the last `max_len - 1` characters
/// prefixed with `…` so the filename is always visible.
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    let start = path.len().saturating_sub(max_len.saturating_sub(1));
    format!("…{}", path.get(start..).unwrap_or(path))
}

/// Format a byte count as a human-readable string (e.g. "215 MB").
pub(crate) fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes >= GB {
        let whole = bytes.checked_div(GB).unwrap_or(0);
        let frac = bytes.wrapping_rem(GB).saturating_mul(10).checked_div(GB).unwrap_or(0);
        format!("{whole}.{frac} GB")
    } else if bytes >= MB {
        format!("{} MB", bytes.checked_div(MB).unwrap_or(0))
    } else if bytes >= KB {
        format!("{} KB", bytes.checked_div(KB).unwrap_or(0))
    } else {
        format!("{bytes} B")
    }
}
