/// Tool implementation for interactive question forms.
mod ask_question;
/// Tool implementation for the `Think` reasoning tool.
mod think;
pub(crate) use think::ThinkState;

use crate::app::panels::Panel;
use crate::infra::tools::{ParamType, ToolDefinition, ToolParam, ToolTexts};
use crate::infra::tools::{ToolResult, ToolUse};
use crate::state::{Kind, State};

use super::Module;

/// Lazily parsed tool text definitions for question tools.
static TOOL_TEXTS: std::sync::LazyLock<ToolTexts> =
    std::sync::LazyLock::new(|| ToolTexts::parse(include_str!("../../../yamls/tools/questions.yaml")));

/// Lazily parsed tool text definitions for core tools (used by `Think`).
static CORE_TOOL_TEXTS: std::sync::LazyLock<ToolTexts> =
    std::sync::LazyLock::new(|| ToolTexts::parse(include_str!("../../../yamls/tools/core.yaml")));

/// Module that provides interactive user question forms.
pub(crate) struct QuestionsModule;

impl Module for QuestionsModule {
    fn id(&self) -> &'static str {
        "questions"
    }
    fn name(&self) -> &'static str {
        "Questions"
    }
    fn description(&self) -> &'static str {
        "Interactive user question forms"
    }
    fn is_core(&self) -> bool {
        true
    }
    fn is_global(&self) -> bool {
        true
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Context", "Manage conversation context and system prompts")]
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let t = &*TOOL_TEXTS;
        let core_t = &*CORE_TOOL_TEXTS;
        vec![
            ToolDefinition::from_yaml("ask_user_question", t)
                .short_desc("Ask user multiple-choice questions")
                .category("Context")
                .param_array(
                    "questions",
                    ParamType::Object(vec![
                        ToolParam::new("question", ParamType::String)
                            .desc("The complete question text. Should be clear, specific, and end with ?")
                            .required(),
                        ToolParam::new("header", ParamType::String)
                            .desc("Very short label (max 12 chars). E.g. \"Auth method\", \"Library\"")
                            .required(),
                        ToolParam::new(
                            "options",
                            ParamType::Array(Box::new(ParamType::Object(vec![
                                ToolParam::new("label", ParamType::String).desc("Display text (1-5 words)").required(),
                                ToolParam::new("description", ParamType::String)
                                    .desc("Explanation of what this option means")
                                    .required(),
                            ]))),
                        )
                        .desc("2-4 available choices. An \"Other\" free-text option is appended automatically.")
                        .required(),
                        ToolParam::new("multiSelect", ParamType::Boolean)
                            .desc("If true, user can select multiple options")
                            .required(),
                    ]),
                    true,
                )
                .build(),
            ToolDefinition::from_yaml("Think", core_t)
                .short_desc("Record a structured reasoning step")
                .category("Context")
                .param("thought_body", ParamType::String, true)
                .build(),
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "ask_user_question" => Some(ask_question::execute(tool, state)),
            "Think" => Some(think::execute(tool, state)),
            _ => None,
        }
    }

    fn create_panel(&self, _context_type: &Kind) -> Option<Box<dyn Panel>> {
        None
    }

    fn dependencies(&self) -> &[&'static str] {
        &[]
    }

    fn init_state(&self, state: &mut State) {
        state.set_ext(ThinkState::default());
    }

    fn reset_state(&self, state: &mut State) {
        state.set_ext(ThinkState::default());
    }

    fn save_module_data(&self, _state: &State) -> serde_json::Value {
        serde_json::Value::Null
    }

    fn load_module_data(&self, _data: &serde_json::Value, _state: &mut State) {}

    fn save_worker_data(&self, state: &State) -> serde_json::Value {
        state
            .get_ext::<ThinkState>()
            .map_or(serde_json::Value::Null, |ts| serde_json::to_value(ts).unwrap_or(serde_json::Value::Null))
    }

    fn load_worker_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Ok(ts) = serde_json::from_value::<ThinkState>(data.clone()) {
            state.set_ext(ts);
        }
    }

    fn pre_flight(&self, _tool: &ToolUse, _state: &State) -> Option<crate::infra::tools::Verdict> {
        None
    }

    fn fixed_panel_types(&self) -> Vec<Kind> {
        vec![]
    }

    fn dynamic_panel_types(&self) -> Vec<Kind> {
        vec![]
    }

    fn fixed_panel_defaults(&self) -> Vec<(Kind, &'static str, bool)> {
        vec![]
    }

    fn context_type_metadata(&self) -> Vec<crate::state::TypeMeta> {
        vec![]
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, super::ToolVisualizer)> {
        vec![]
    }

    fn context_display_name(&self, _context_type: &str) -> Option<&'static str> {
        None
    }

    fn context_detail(&self, _ctx: &crate::state::Entry) -> Option<String> {
        None
    }

    fn overview_context_section(&self, _state: &State) -> Option<String> {
        None
    }

    fn overview_render_sections(&self, _state: &State) -> Vec<(u8, Vec<cp_render::Block>)> {
        vec![]
    }

    fn on_close_context(&self, _ctx: &crate::state::Entry, _state: &mut State) -> Option<Result<String, String>> {
        None
    }

    fn on_user_message(&self, _state: &mut State) {}

    fn on_stream_stop(&self, _state: &mut State) {}

    fn on_tool_progress(&self, _tool_name: &str, _input_so_far: &str, _state: &mut State) {}

    fn on_tool_complete(&self, tool_name: &str, state: &mut State) {
        if tool_name == "Think" {
            return;
        }
        // Non-Think tool: drift counter toward (and below) zero
        let fire = {
            let ts = state.ext_mut::<ThinkState>();
            ts.consecutive_count = ts.consecutive_count.saturating_sub(1).min(0);
            // Check if we've hit the next notification point
            if ts.consecutive_count == ts.next_notification_at {
                ts.next_notification_at = ts.next_notification_at.saturating_add(ts.reminder_threshold);
                true
            } else {
                false
            }
        };
        if fire {
            let _id = cp_mod_spine::types::SpineState::create_notification(
                state,
                cp_mod_spine::types::NotificationType::Custom,
                "Think Reminder".into(),
                "Please think more. Thinking is both cheap in tokens, and drastically \
                 augments your performances. Make a habit out of it."
                    .into(),
            );
        }
    }

    fn watch_paths(&self, _state: &State) -> Vec<cp_base::panels::WatchSpec> {
        vec![]
    }

    fn should_invalidate_on_fs_change(
        &self,
        _ctx: &crate::state::Entry,
        _changed_path: &str,
        _is_dir_event: bool,
    ) -> bool {
        false
    }

    fn watcher_immediate_refresh(&self) -> bool {
        true
    }
}
