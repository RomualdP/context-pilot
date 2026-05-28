//! Queue flush execution — dequeues and runs all queued tool calls.
//!
//! Extracted from `cleanup.rs` to keep that module under the 500-line limit.

use crate::app::App;
use crate::app::panels::now_ms;
use crate::infra::tools::execute_tool;
use crate::state::{Message, MsgKind, MsgStatus, State, ToolUseRecord};

use cp_mod_queue::types::QueueState;

use std::fmt::Write as _;

/// Flushed tool execution pair: the original `ToolUse` and its result.
pub(crate) struct FlushedTool {
    /// The original tool-use request that was dequeued and executed.
    pub tool: cp_base::tools::ToolUse,
    /// The execution result for this tool call.
    pub result: crate::infra::tools::ToolResult,
    /// The queue position this tool occupied (for compact display).
    pub queue_index: usize,
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
        let result = execute_tool(&queued_tool, state);
        let status = if result.is_error { "ERROR" } else { "ok" };
        let short = if result.content.len() > 100 {
            let end = result.content.floor_char_boundary(97);
            format!("{}...", result.content.get(..end).unwrap_or(""))
        } else {
            result.content.clone()
        };
        let _r = writeln!(summary, "{}. {} → {} ({})", call.index, call.tool_name, status, short);
        flushed.push(FlushedTool { tool: queued_tool, result, queue_index: call.index });
    }

    // The summary wrapper preserves tempo — only the individual flushed
    // tool results should drive the tempo decision (transparent queue).
    let mut wrapper = crate::infra::tools::ToolResult::new(tool.id.clone(), summary, false);
    wrapper.preserves_tempo = true;
    (wrapper, flushed)
}

/// Create and persist a compact `tool_call` message for a queue-flushed `ToolUse`.
///
/// Instead of replaying the full parameters (which duplicate the already-visible
/// "Queued as #N" message), this saves a lightweight `Tool_execution` stub with
/// just the tool name, queue position, and parameter byte-size.
pub(crate) fn save_flushed_tool_call_message(app: &mut App, tool: &cp_base::tools::ToolUse, queue_index: usize) {
    let tool_id = format!("T{}", app.state.next_tool_id);
    let tool_global_uid = format!("UID_{}_T", app.state.global_next_uid);
    app.state.next_tool_id = app.state.next_tool_id.saturating_add(1);
    app.state.global_next_uid = app.state.global_next_uid.saturating_add(1);

    let params_size = serde_json::to_string(&tool.input).map_or(0, |s| s.len());
    let compact_input = serde_json::json!({
        "tool_name": tool.name,
        "tool_position": queue_index,
        "tool_parameters_size": params_size,
    });

    let tool_msg = Message {
        id: tool_id,
        uid: Some(tool_global_uid),
        role: "assistant".to_string(),
        msg_type: MsgKind::ToolCall,
        content: String::new(),
        content_token_count: 0,
        status: MsgStatus::Full,
        tool_uses: vec![ToolUseRecord {
            id: tool.id.clone(),
            name: "Tool_execution".to_string(),
            input: compact_input,
        }],
        tool_results: Vec::new(),
        input_tokens: 0,
        timestamp_ms: now_ms(),
    };
    app.save_message_async(&tool_msg);
    app.state.messages.push(tool_msg);
}
