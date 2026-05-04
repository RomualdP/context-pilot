//! Send, reply, edit, and delete message operations.
//!
//! Extracted from `tools/mod.rs` to keep it under the 500-line structure limit.

use cp_base::state::runtime::State;
use cp_base::tools::{ToolResult, ToolUse};

use crate::client;

use super::helpers::{clear_report_here, record_sent_message, resolve_event_ref, resolve_room_param};

/// `Chat_send` — send, reply, edit, or delete a message.
///
/// Unified endpoint: exactly one of `message`, `edit`, or `delete` must
/// be provided. `reply_to` pairs with `message` for threaded replies.
/// Default message type is `m.notice`; set `notice: false` for `m.text`.
pub(crate) fn execute_send(tool: &ToolUse, state: &mut State) -> ToolResult {
    let room_input = tool.input.get("room").and_then(serde_json::Value::as_str).unwrap_or("#general");

    let room_id = match resolve_room_param(room_input, state) {
        Ok(id) => id,
        Err(e) => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: e,
                display: None,
                tldr: None,
                is_error: true,
                preserves_tempo: false,
                tool_name: tool.name.clone(),
            };
        }
    };

    let message = tool.input.get("message").and_then(serde_json::Value::as_str);
    let reply_to = tool.input.get("reply_to").and_then(serde_json::Value::as_str);
    let edit_ref = tool.input.get("edit").and_then(serde_json::Value::as_str);
    let delete_ref = tool.input.get("delete").and_then(serde_json::Value::as_str);
    let is_notice = tool.input.get("notice").and_then(serde_json::Value::as_bool).unwrap_or(true);
    let report_later = tool.input.get("report_later_here").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let image_path = tool.input.get("image").and_then(serde_json::Value::as_str);

    // Image upload path — send a local file as m.image
    if let Some(img_path) = image_path {
        return match client::send::send_image(&room_id, img_path) {
            Ok(event_id) => {
                if !report_later {
                    clear_report_here(state, &room_id);
                }
                ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Image '{img_path}' sent to '{room_input}' (event: {event_id})."),
                    display: None,
                    tldr: None,
                    is_error: false,
                    preserves_tempo: false,
                    tool_name: tool.name.clone(),
                }
            }
            Err(e) => ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Image send failed: {e}"),
                display: None,
                tldr: None,
                is_error: true,
                preserves_tempo: false,
                tool_name: tool.name.clone(),
            },
        };
    }

    // Delete path
    if let Some(ref_str) = delete_ref {
        return execute_delete(tool, state, &room_id, ref_str);
    }

    // Edit path
    if let Some(ref_str) = edit_ref {
        let body = message.unwrap_or("");
        if body.is_empty() {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Edit requires a 'message' with the new content.".to_string(),
                display: None,
                tldr: None,
                is_error: true,
                preserves_tempo: false,
                tool_name: tool.name.clone(),
            };
        }
        return execute_edit(tool, state, &room_id, (ref_str, body));
    }

    // Send / Reply path
    let Some(body) = message else {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "Provide 'message', 'edit', or 'delete'.".to_string(),
            display: None,
            tldr: None,
            is_error: true,
            preserves_tempo: false,
            tool_name: tool.name.clone(),
        };
    };

    // Empty message = silent opt-out from report_here (send nothing)
    if body.is_empty() {
        clear_report_here(state, &room_id);
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Acknowledged '{room_input}' — removed from pending responses."),
            display: None,
            tldr: None,
            is_error: false,
            preserves_tempo: false,
            tool_name: tool.name.clone(),
        };
    }

    if let Some(reply_ref) = reply_to {
        // Resolve the short ref to a full event ID
        let event_id = resolve_event_ref(state, &room_id, reply_ref);
        let Some(event_id) = event_id else {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Cannot resolve reply_to ref '{reply_ref}'. Use an E<n> ref from the room panel."),
                display: None,
                tldr: None,
                is_error: true,
                preserves_tempo: false,
                tool_name: tool.name.clone(),
            };
        };
        match client::send::send_reply(&room_id, body, &event_id, is_notice) {
            Ok(new_event_id) => {
                if !report_later {
                    clear_report_here(state, &room_id);
                }
                record_sent_message(state, &room_id, body);
                ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Reply sent to {reply_ref} in '{room_input}' (event: {new_event_id})."),
                    display: None,
                    tldr: None,
                    is_error: false,
                    preserves_tempo: false,
                    tool_name: tool.name.clone(),
                }
            }
            Err(e) => ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Reply failed: {e}"),
                display: None,
                tldr: None,
                is_error: true,
                preserves_tempo: false,
                tool_name: tool.name.clone(),
            },
        }
    } else {
        match client::send::send_message(&room_id, body, is_notice) {
            Ok(new_event_id) => {
                if !report_later {
                    clear_report_here(state, &room_id);
                }
                record_sent_message(state, &room_id, body);
                ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Message sent to '{room_input}' (event: {new_event_id})."),
                    display: None,
                    tldr: None,
                    is_error: false,
                    preserves_tempo: false,
                    tool_name: tool.name.clone(),
                }
            }
            Err(e) => ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Send failed: {e}"),
                display: None,
                tldr: None,
                is_error: true,
                preserves_tempo: false,
                tool_name: tool.name.clone(),
            },
        }
    }
}

/// Delete (redact) a message by short ref.
fn execute_delete(tool: &ToolUse, state: &State, room_id: &str, ref_str: &str) -> ToolResult {
    let event_id = resolve_event_ref(state, room_id, ref_str);
    let Some(event_id) = event_id else {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Cannot resolve delete ref '{ref_str}'."),
            display: None,
            tldr: None,
            is_error: true,
            preserves_tempo: false,
            tool_name: tool.name.clone(),
        };
    };
    match client::send::redact_message(room_id, &event_id, Some("Deleted by Context Pilot")) {
        Ok(()) => ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Message {ref_str} deleted."),
            display: None,
            tldr: None,
            is_error: false,
            preserves_tempo: false,
            tool_name: tool.name.clone(),
        },
        Err(e) => ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Delete failed: {e}"),
            display: None,
            tldr: None,
            is_error: true,
            preserves_tempo: false,
            tool_name: tool.name.clone(),
        },
    }
}

/// Edit a message by short ref with replacement content.
fn execute_edit(tool: &ToolUse, state: &State, room_id: &str, edit_ctx: (&str, &str)) -> ToolResult {
    let (ref_str, new_body) = edit_ctx;
    let event_id = resolve_event_ref(state, room_id, ref_str);
    let Some(event_id) = event_id else {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Cannot resolve edit ref '{ref_str}'."),
            display: None,
            tldr: None,
            is_error: true,
            preserves_tempo: false,
            tool_name: tool.name.clone(),
        };
    };
    match client::send::edit_message(room_id, &event_id, new_body) {
        Ok(new_event_id) => ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Message {ref_str} edited (new event: {new_event_id})."),
            display: None,
            tldr: None,
            is_error: false,
            preserves_tempo: false,
            tool_name: tool.name.clone(),
        },
        Err(e) => ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Edit failed: {e}"),
            display: None,
            tldr: None,
            is_error: true,
            preserves_tempo: false,
            tool_name: tool.name.clone(),
        },
    }
}
