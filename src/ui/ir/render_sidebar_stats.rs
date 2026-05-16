//! Token statistics rendering for the sidebar.
//!
//! Extracted from `render_sidebar.rs` to stay within the 500-line limit.
//! Renders the hit/miss/output table, cache breakpoint gauge, and total
//! cost — wrapped in rounded borders (╭╮╰╯).

use cp_render::frame::TokenStats;
use ratatui::prelude::{Line, Span, Style};
use unicode_width::UnicodeWidthStr as _;

use crate::ui::{chars, helpers::format_number, theme};
use cp_base::cast::Safe as _;

use super::render_sidebar::padded;

/// Render the token statistics table from IR, wrapped in rounded borders.
pub(super) fn render_token_stats(lines: &mut Vec<Line<'static>>, stats: &TokenStats, cw: usize) {
    use crate::ui::helpers::{Cell, render_table};

    let border_style = Style::default().fg(theme::border_muted());
    let inner_width = cw.saturating_sub(2); // space between │ and │

    let format_cost = |cost: Option<f64>| -> String {
        cost.map_or(String::new(), |c| {
            if c < 0.01 {
                format!("${c:.3}")
            } else if c < 1.0 {
                format!("${c:.2}")
            } else {
                format!("${c:.1}")
            }
        })
    };

    // ── Build content lines (no indent — borders handle alignment) ───

    let mut content: Vec<Line<'static>> = Vec::new();

    let hit_icon = chars::ARROW_UP.to_string();
    let miss_icon = chars::CROSS.to_string();
    let out_icon = chars::ARROW_DOWN.to_string();

    let header_cells = [
        Cell::new("", Style::default()),
        Cell::right(format!("{hit_icon} hit"), Style::default().fg(theme::success())),
        Cell::right(format!("{miss_icon} miss"), Style::default().fg(theme::warning())),
        Cell::right(format!("{out_icon} out"), Style::default().fg(theme::accent_dim())),
    ];

    let mut rows: Vec<Vec<Cell>> = Vec::new();

    for row in &stats.rows {
        rows.push(vec![
            Cell::new(&row.label, Style::default().fg(theme::text_muted())),
            Cell::right(format_number(row.hit.to_usize()), Style::default().fg(theme::success())),
            Cell::right(format_number(row.miss.to_usize()), Style::default().fg(theme::warning())),
            Cell::right(format_number(row.output.to_usize()), Style::default().fg(theme::accent_dim())),
        ]);

        let hit_cost = format_cost(row.hit_cost);
        let miss_cost = format_cost(row.miss_cost);
        let out_cost = format_cost(row.output_cost);

        if !hit_cost.is_empty() || !miss_cost.is_empty() || !out_cost.is_empty() {
            rows.push(vec![
                Cell::new("", Style::default()),
                Cell::right(hit_cost, Style::default().fg(theme::text_muted())),
                Cell::right(miss_cost, Style::default().fg(theme::text_muted())),
                Cell::right(out_cost, Style::default().fg(theme::text_muted())),
            ]);
        }
    }

    content.extend(render_table(&header_cells, &rows, None, 0));

    // Uncached input tokens
    if stats.uncached_input > 0 {
        content.push(Line::from(vec![Span::styled(
            format!("uncached: {}", format_number(stats.uncached_input.to_usize())),
            Style::default().fg(theme::error()),
        )]));
    }

    // Alive cache breakpoints
    if stats.alive_breakpoints > 0 {
        content.push(Line::from(vec![Span::styled(
            format!("alive BPs: {}", stats.alive_breakpoints),
            Style::default().fg(theme::success()),
        )]));

        if !stats.alive_bp_positions.is_empty() {
            let gauge_width = inner_width;
            let mut gauge_spans = Vec::new();
            for i in 0..gauge_width {
                let col_permille_start = i.saturating_mul(1000).checked_div(gauge_width).unwrap_or(0);
                let col_permille_end = (i.saturating_add(1)).saturating_mul(1000).checked_div(gauge_width).unwrap_or(0);
                let has_bp = stats
                    .alive_bp_positions
                    .iter()
                    .any(|&p| usize::from(p) >= col_permille_start && usize::from(p) < col_permille_end);
                if has_bp {
                    gauge_spans.push(Span::styled("|", Style::default().fg(theme::success())));
                } else {
                    gauge_spans.push(Span::styled(chars::BLOCK_LIGHT, Style::default().fg(theme::bg_elevated())));
                }
            }
            content.push(Line::from(gauge_spans));
        }
    }

    // Total cost
    if let Some(total) = stats.total_cost {
        let total_str = if total < 0.01 { format!("${total:.3}") } else { format!("${total:.2}") };
        content.push(Line::from(vec![Span::styled(
            format!("total: {total_str}"),
            Style::default().fg(theme::text_muted()),
        )]));
    }

    // ── Wrap content in rounded borders ──────────────────────────────

    // Top border: ╭───...───╮
    lines.push(padded(vec![
        Span::styled("╭", border_style),
        Span::styled("─".repeat(inner_width), border_style),
        Span::styled("╮", border_style),
    ]));

    // Content lines: │ content ... │
    for content_line in content {
        let line_width: usize = content_line.spans.iter().map(|s| s.content.width()).sum();
        let pad = inner_width.saturating_sub(line_width);
        let mut spans = Vec::with_capacity(content_line.spans.len().saturating_add(4));
        spans.push(Span::raw(" ")); // structural 1-char indent
        spans.push(Span::styled("│", border_style));
        spans.extend(content_line.spans);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled("│", border_style));
        lines.push(Line::from(spans));
    }

    // Bottom border: ╰───...───╯
    lines.push(padded(vec![
        Span::styled("╰", border_style),
        Span::styled("─".repeat(inner_width), border_style),
        Span::styled("╯", border_style),
    ]));
}
