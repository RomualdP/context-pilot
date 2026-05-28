//! IR block builders for the Tools / Configuration panel.
//!
//! Produces `Vec<Block>` equivalents of `render_details.rs` functions
//! (`render_tools`, `render_seeds`), using `cp_render`
//! types instead of ratatui. Called from `ToolsPanel::blocks()`.

use std::collections::HashSet;

use cp_render::{Align, Block, Cell, Column, Semantic, Span};

use crate::modules::all_modules;
use crate::state::State;

/// TOOLS section: tools grouped by category, with enable/disable status.
pub(super) fn tools_blocks(state: &State) -> Vec<Block> {
    let mut out = Vec::new();

    let enabled_count = state.tools.iter().filter(|t| t.enabled).count();
    let disabled_count = state.tools.iter().filter(|t| !t.enabled).count();

    out.push(Block::Header(vec![
        Span::styled("TOOLS".to_owned(), Semantic::Muted),
        Span::muted(format!("  ({enabled_count} enabled, {disabled_count} disabled)")),
    ]));
    out.push(Block::Empty);

    // Build category descriptions from modules
    let cat_descs: std::collections::HashMap<&str, &str> =
        all_modules().iter().flat_map(|m| m.tool_category_descriptions()).collect();

    // Collect unique categories in order of first appearance
    let mut seen_cats = HashSet::new();
    let categories: Vec<String> =
        state.tools.iter().filter(|t| seen_cats.insert(t.category.clone())).map(|t| t.category.clone()).collect();

    for category in &categories {
        let category_tools: Vec<_> = state.tools.iter().filter(|t| t.category == *category).collect();
        if category_tools.is_empty() {
            continue;
        }

        let cat_name = category.to_uppercase();
        let cat_desc = cat_descs.get(category.as_str()).copied().unwrap_or("");

        out.push(Block::line(vec![Span::accent(format!(" {cat_name}")).bold(), Span::muted(format!("  {cat_desc}"))]));

        let columns = vec![
            Column { header: "Tool".to_owned(), align: Align::Left },
            Column { header: "On".to_owned(), align: Align::Left },
            Column { header: "Description".to_owned(), align: Align::Left },
        ];

        let rows: Vec<Vec<Cell>> = category_tools
            .iter()
            .map(|tool| {
                let (status_icon, status_semantic) =
                    if tool.enabled { ("\u{2713}", Semantic::Success) } else { ("\u{2717}", Semantic::Error) };
                vec![
                    Cell::styled(tool.id.clone(), Semantic::Default),
                    Cell::styled(status_icon.to_owned(), status_semantic),
                    Cell::styled(tool.short_desc.clone(), Semantic::Muted),
                ]
            })
            .collect();

        out.push(Block::Table { columns, rows });
        out.push(Block::Empty);
    }

    out
}
