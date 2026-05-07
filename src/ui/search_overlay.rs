//! Ctrl+I Meilisearch indexing status overlay.
//!
//! Renders a floating, centered info box showing the Meilisearch server
//! status, indexing metrics, and queue depth.

use ratatui::Frame;
use ratatui::prelude::{Rect, Style};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::state::State;
use crate::ui::theme;

/// Overlay width and height in terminal cells.
const OVERLAY_WIDTH: u16 = 50;
const OVERLAY_HEIGHT: u16 = 14;

/// Render the Meilisearch indexing status overlay.
///
/// Displays server status, index metrics, queue depth, and error count
/// in a centered, bordered box.
pub(crate) fn render_index_overlay(frame: &mut Frame<'_>, state: &State, area: Rect) {
    let popup = centered_rect(OVERLAY_WIDTH, OVERLAY_HEIGHT, area);
    let lines = build_overlay_lines(state);

    let block = Block::default()
        .title(" Indexing Status ")
        .borders(Borders::ALL)
        .style(Style::default().bg(theme::bg_base()).fg(theme::text()));

    let paragraph = Paragraph::new(lines).block(block);

    frame.render_widget(Clear, popup);
    frame.render_widget(paragraph, popup);
}

/// Build the overlay content lines from the search module's state.
fn build_overlay_lines(state: &State) -> Vec<Line<'static>> {
    let Some(info) = cp_mod_search::overlay_info(state) else {
        return vec![
            Line::from(""),
            Line::from("  Search module not initialized."),
            Line::from(""),
            Line::from(dim_span("  Press Ctrl+I or Esc to dismiss")),
        ];
    };

    let server_url = format!("http://127.0.0.1:{}", info.port);
    let status = if info.port > 0 { "● online" } else { "○ offline" };
    let status_color = if info.port > 0 { theme::success() } else { theme::error() };
    let last_activity = if info.last_activity_ms > 0 { format_ago(info.last_activity_ms) } else { "never".to_string() };
    let ready_label = if info.index_ready { "Ready" } else { "Scanning…" };

    vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Server:   "),
            Span::styled(server_url, Style::default().fg(theme::text())),
            Span::raw(" "),
            Span::styled(status, Style::default().fg(status_color)),
        ]),
        Line::from(""),
        Line::from(format!("  Files:    {} chunks ({} files)", info.chunks_indexed, info.files_indexed,)),
        Line::from(format!("  Queue:    {} pending", info.queue_depth)),
        Line::from(format!("  Errors:   {}", info.error_count)),
        Line::from(format!("  Last:     {last_activity}")),
        Line::from(format!("  Status:   {ready_label}")),
        Line::from(""),
        Line::from(""),
        Line::from(dim_span("  Press Ctrl+I or Esc to dismiss")),
    ]
}

/// Compute a centered rectangle within the given area.
const fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let eff_w = if width > area.width { area.width } else { width };
    let eff_h = if height > area.height { area.height } else { height };
    let x_off = (area.width - eff_w) / 2;
    let y_off = (area.height - eff_h) / 2;
    Rect::new(area.x + x_off, area.y + y_off, eff_w, eff_h)
}

/// Format a millisecond timestamp as a relative "X ago" string.
fn format_ago(ms_then: u64) -> String {
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
    let now_u64 = u64::try_from(now_ms).unwrap_or(u64::MAX);
    let diff_sec = now_u64.saturating_sub(ms_then) / 1000;
    if diff_sec < 60 {
        format!("{diff_sec}s ago")
    } else if diff_sec < 3600 {
        format!("{}m ago", diff_sec / 60)
    } else {
        format!("{}h ago", diff_sec / 3600)
    }
}

/// Create a dimmed span for hint text.
fn dim_span(text: &'static str) -> Span<'static> {
    Span::styled(text, Style::default().add_modifier(Modifier::DIM))
}
