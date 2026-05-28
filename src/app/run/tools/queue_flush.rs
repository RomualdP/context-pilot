//! Queue flush execution — dequeues and runs all queued tool calls.
//!
//! Extracted from `cleanup.rs` to keep that module under the 500-line limit.

use crate::infra::tools::execute_tool;
use crate::state::State;

use cp_mod_queue::types::QueueState;

use std::fmt::Write as _;

/// Flushed tool execution pair: the original `ToolUse` and its result.
pub(crate) struct FlushedTool {
    /// The original tool-use request that was dequeued and executed.
    pub tool: cp_base::tools::ToolUse,
    /// The execution result for this tool call.
    pub result: crate::infra::tools::ToolResult,
}

/// Execute all queued tool calls in order.
/// Returns (`summary_result`, `flushed_tools`) so the pipeline can run callbacks/sentinels
/// on the individual tools — not just the `Queue_execute` wrapper.
pub(crate) fn execute_queue_flush(
    tool: &cp_base::tools::ToolUse,
    state: &mut State,
) -> (crate::infra::tools::ToolResult, Vec<FlushedTool>) {
    let qs = QueueState::get_mut(state);
    if qs.queued_calls.is_empty() {
        return (
            crate::infra::tools::ToolResult::new(
                tool.id.clone(),
                "Queue is empty — nothing to execute.".to_string(),
                false,
            ),
            Vec::new(),
        );
    }
    let calls = qs.flush();
    qs.active = false;

    let mut summary = format!("Executed {} queued action(s):\n", calls.len());
    let mut flushed = Vec::with_capacity(calls.len());

    for call in &calls {
        // Generate a fresh tool_use_id to avoid collision with the intercept-time message.
        // The original id was already used in the "Queued as #N" tool_result at intercept time.
        let fresh_id = format!("flush_{}_{}", call.index, call.tool_use_id);
        let queued_tool =
            cp_base::tools::ToolUse { id: fresh_id, name: call.tool_name.clone(), input: call.input.clone() };
        let flush_start = std::time::Instant::now();
        let result = execute_tool(&queued_tool, state);
        crate::infra::profiler::log_slow_tool(&queued_tool.name, flush_start.elapsed());
        let status = if result.is_error { "ERROR" } else { "ok" };
        let short = if result.content.len() > 100 {
            let end = result.content.floor_char_boundary(97);
            format!("{}...", result.content.get(..end).unwrap_or(""))
        } else {
            result.content.clone()
        };
        let _r = writeln!(summary, "{}. {} → {} ({})", call.index, call.tool_name, status, short);
        flushed.push(FlushedTool { tool: queued_tool, result });
    }

    // The summary wrapper preserves tempo — only the individual flushed
    // tool results should drive the tempo decision (transparent queue).
    let mut wrapper = crate::infra::tools::ToolResult::new(tool.id.clone(), summary, false);
    wrapper.preserves_tempo = true;
    (wrapper, flushed)
}
