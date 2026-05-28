//! Context panel navigation — next/prev panel and page-based jumping.

use crate::state::State;

use super::config;
use super::helpers::switch_to_panel;

/// Maximum dynamic entries per sidebar page (must match `render_sidebar.rs`).
const DYNAMIC_PAGE_SIZE: usize = 10;

/// Navigate to the next (`forward=true`) or previous (`forward=false`) context panel,
/// sorted by numeric panel ID.
pub(super) fn select_context(state: &mut State, forward: bool) {
    if state.context.is_empty() {
        return;
    }
    let mut sorted: Vec<usize> = (0..state.context.len()).collect();
    sorted.sort_by(|&a, &b| {
        let id_a = state
            .context
            .get(a)
            .and_then(|el| el.id.strip_prefix('P'))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        let id_b = state
            .context
            .get(b)
            .and_then(|el| el.id.strip_prefix('P'))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        id_a.cmp(&id_b)
    });
    let cur = sorted.iter().position(|&i| i == state.selected_context).unwrap_or(0);
    let next = if forward {
        config::wrap_next(cur, sorted.len())
    } else if cur == 0 {
        sorted.len().saturating_sub(1)
    } else {
        cur.saturating_sub(1)
    };
    let Some(&selected) = sorted.get(next) else { return };
    switch_to_panel(state, selected);
}

/// Jump to the first dynamic panel on the next or previous page.
///
/// - From a **fixed** panel: forward → last page start, backward → first page start.
/// - From a **dynamic** panel: forward/backward wraps circularly through pages.
pub(super) fn page_dynamic(state: &mut State, forward: bool) {
    if state.context.is_empty() {
        return;
    }
    // Sort indices by panel ID numerically (same ordering as select_context / sidebar).
    let mut sorted: Vec<usize> = (0..state.context.len()).collect();
    sorted.sort_by(|&a, &b| {
        let id_a = state
            .context
            .get(a)
            .and_then(|el| el.id.strip_prefix('P'))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        let id_b = state
            .context
            .get(b)
            .and_then(|el| el.id.strip_prefix('P'))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        id_a.cmp(&id_b)
    });

    // Collect dynamic panel indices only (preserving sorted order).
    let dynamic_indices: Vec<usize> =
        sorted.iter().filter(|&&i| state.context.get(i).is_some_and(|c| !c.context_type.is_fixed())).copied().collect();

    if dynamic_indices.is_empty() {
        return;
    }

    let total_pages = dynamic_indices.len().div_ceil(DYNAMIC_PAGE_SIZE);

    // Is the currently selected panel dynamic?
    let current_is_dynamic = state.context.get(state.selected_context).is_some_and(|c| !c.context_type.is_fixed());

    let target_page = if current_is_dynamic {
        // Find which page the current selection is on, then move to next/prev.
        let pos = dynamic_indices.iter().position(|&i| i == state.selected_context).unwrap_or(0);
        let current_page = pos.checked_div(DYNAMIC_PAGE_SIZE).unwrap_or(0);
        if forward {
            if current_page >= total_pages.saturating_sub(1) { 0 } else { current_page.saturating_add(1) }
        } else if current_page == 0 {
            total_pages.saturating_sub(1)
        } else {
            current_page.saturating_sub(1)
        }
    } else {
        // From a fixed panel: forward → last page, backward → first page.
        if forward { total_pages.saturating_sub(1) } else { 0 }
    };

    // Jump to the first panel on the target page.
    let target_idx = target_page.saturating_mul(DYNAMIC_PAGE_SIZE);
    if let Some(&selected) = dynamic_indices.get(target_idx) {
        switch_to_panel(state, selected);
    }
}
