//! Conversation and overlay IR types.
//!
//! These types model the conversation region (message history, streaming
//! tool calls, input area) and modal overlays (question forms,
//! autocomplete popups).

use serde::Serialize;

use crate::{Block, Semantic};

// ── Conversation ─────────────────────────────────────────────────────

/// The conversation region — message history + input area.
#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    /// Collapsed history sections (previous conversations).
    pub history_sections: Vec<HistorySection>,
    /// Visible messages.
    pub messages: Vec<Message>,
    /// Currently streaming tool calls.
    pub streaming_tools: Vec<StreamingTool>,
    /// Input area at the bottom.
    pub input: InputArea,
}

/// A collapsed history section header.
#[derive(Debug, Clone, Serialize)]
pub struct HistorySection {
    /// Display label (e.g. "History (23 messages)").
    pub label: String,
    /// Whether this section is expanded.
    pub expanded: bool,
    /// Messages inside this section (only present when expanded).
    pub messages: Vec<Message>,
}

/// A single conversation message.
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    /// Role: "user", "assistant", "system".
    pub role: String,
    /// Content blocks (rendered as IR blocks, not raw markdown).
    pub content: Vec<Block>,
    /// Tool use previews attached to this message.
    pub tool_uses: Vec<ToolUsePreview>,
    /// Tool result previews attached to this message.
    pub tool_results: Vec<ToolResultPreview>,
}

/// Preview of a tool use (collapsed in conversation view).
#[derive(Debug, Clone, Serialize)]
pub struct ToolUsePreview {
    /// Tool name (e.g. `Edit`, `console_easy_bash`).
    pub tool_name: String,
    /// Short summary (e.g. "src/main.rs: 3 lines changed").
    pub summary: String,
    /// Semantic colour (success/error/info based on result).
    pub semantic: Semantic,
}

/// Preview of a tool result (collapsed in conversation view).
#[derive(Debug, Clone, Serialize)]
pub struct ToolResultPreview {
    /// Tool name.
    pub tool_name: String,
    /// Short result summary.
    pub summary: String,
    /// Whether the tool call succeeded.
    pub success: bool,
}

/// A tool call currently being streamed.
#[derive(Debug, Clone, Serialize)]
pub struct StreamingTool {
    /// Tool name.
    pub tool_name: String,
    /// Partial input JSON accumulated so far.
    pub partial_input: String,
}

/// The input area at the bottom of the conversation.
#[derive(Debug, Clone, Serialize)]
pub struct InputArea {
    /// Current input text.
    pub text: String,
    /// Cursor position (byte offset).
    pub cursor: usize,
    /// Placeholder text when input is empty.
    pub placeholder: String,
    /// Whether input is currently focused.
    pub focused: bool,
}

// ── Overlays ─────────────────────────────────────────────────────────

/// A modal overlay rendered on top of the main UI.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum Overlay {
    /// Multiple-choice question form.
    QuestionForm(QuestionForm),
    /// File path autocomplete popup.
    Autocomplete(Autocomplete),
    /// Performance monitoring overlay (F12).
    Perf(PerfOverlay),
}

/// A question form overlay (`ask_user_question`).
#[derive(Debug, Clone, Serialize)]
pub struct QuestionForm {
    /// Questions to display.
    pub questions: Vec<Question>,
    /// Index of the currently focused question.
    pub focused_index: usize,
}

/// A single question in the form.
#[derive(Debug, Clone, Serialize)]
pub struct Question {
    /// Short header label.
    pub header: String,
    /// Full question text.
    pub text: String,
    /// Available options.
    pub options: Vec<QuestionOption>,
    /// Whether multiple selections are allowed.
    pub multi_select: bool,
    /// Indices of currently selected options.
    pub selected: Vec<usize>,
    /// Free-text "Other" input value.
    pub other_text: String,
}

/// A single option in a question.
#[derive(Debug, Clone, Serialize)]
pub struct QuestionOption {
    /// Display label.
    pub label: String,
    /// Description text.
    pub description: String,
}

/// Performance monitoring overlay (F12).
#[derive(Debug, Clone, Serialize)]
pub struct PerfOverlay {
    /// Frames per second.
    pub fps: f64,
    /// Average frame time in milliseconds.
    pub frame_avg_ms: f64,
    /// Maximum frame time in milliseconds.
    pub frame_max_ms: f64,
    /// Semantic colour for frame time (green/yellow/red).
    pub frame_semantic: Semantic,
    /// CPU usage percentage (0–100).
    pub cpu_usage: f32,
    /// Semantic colour for CPU usage.
    pub cpu_semantic: Semantic,
    /// Memory usage in megabytes.
    pub memory_mb: f64,
    /// Optional Meilisearch process stats.
    pub meili: Option<PerfMeiliStats>,
    /// Budget bars (e.g. 60fps, 30fps).
    pub budget_bars: Vec<PerfBudgetBar>,
    /// Recent frame times for sparkline (milliseconds).
    pub sparkline: Vec<f64>,
    /// Top operations sorted by cumulative time.
    pub operations: Vec<PerfOp>,
}

/// Meilisearch process stats for perf overlay.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PerfMeiliStats {
    /// CPU usage percentage.
    pub cpu_pct: f64,
    /// Semantic colour for CPU usage.
    pub cpu_semantic: Semantic,
    /// Memory usage in megabytes.
    pub memory_mb: f64,
}

/// A budget bar in the perf overlay.
#[derive(Debug, Clone, Serialize)]
pub struct PerfBudgetBar {
    /// Label (e.g. "60fps", "30fps").
    pub label: String,
    /// Current usage as percentage of budget (0–150).
    pub percent: f64,
    /// Semantic colour (green/yellow/red).
    pub semantic: Semantic,
}

/// A single operation row in the perf overlay table.
#[derive(Debug, Clone, Serialize)]
pub struct PerfOp {
    /// Operation name.
    pub name: String,
    /// Mean execution time in milliseconds.
    pub mean_ms: f64,
    /// Semantic colour for mean time.
    pub mean_semantic: Semantic,
    /// Standard deviation in milliseconds.
    pub std_ms: f64,
    /// Semantic colour for std deviation.
    pub std_semantic: Semantic,
    /// Pre-formatted cumulative time (e.g. "1.2s", "450ms").
    pub total_display: String,
    /// Whether this operation is a hotspot (>30% of total).
    pub is_hotspot: bool,
}

/// File path autocomplete popup.
#[derive(Debug, Clone, Serialize)]
pub struct Autocomplete {
    /// Current query / prefix.
    pub query: String,
    /// Matching entries.
    pub entries: Vec<AutocompleteEntry>,
    /// Index of the highlighted entry.
    pub selected_index: usize,
}

/// A single autocomplete suggestion.
#[derive(Debug, Clone, Serialize)]
pub struct AutocompleteEntry {
    /// Display text (file name or path).
    pub label: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// Icon character.
    pub icon: String,
}
