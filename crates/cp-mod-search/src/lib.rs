//! Meilisearch-powered search module for Context Pilot.
//!
//! Provides full-text search across project files and logs via an embedded
//! Meilisearch server. Files are chunked using tree-sitter AST parsing
//! (with a fixed-size fallback) and indexed in the background.
//!
//! One tool: `search` — queries both file and log indexes.
//! Results appear as dynamic search result panels.

/// Configuration constants: extension allowlists, path exclusions, size limits.
pub mod config;
/// Core data types: `SearchState`, `SearchPersistData`, etc.
pub mod types;

use cp_base::modules::Module;
use cp_base::panels::Panel;
use cp_base::state::context::{Kind, TypeMeta};
use cp_base::state::runtime::State;
use cp_base::tools::{ToolDefinition, ToolResult, ToolUse};

use types::{SearchPersistData, SearchState};

/// Compute an 8-character hex hash of a path for per-project index naming.
fn hash_project_path(path: &str) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(path.as_bytes());
    // Take first 4 bytes → 8 hex chars
    hex_encode_4_bytes(hash.as_slice())
}

/// Encode the first 4 bytes of a slice as an 8-character lowercase hex string.
fn hex_encode_4_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(8);
    for &b in bytes.iter().take(4) {
        use std::fmt::Write;
        let _r = write!(out, "{b:02x}");
    }
    out
}

/// Meilisearch-powered search module.
///
/// Manages an embedded Meilisearch server, background file indexer,
/// and a unified `search` tool for querying project files and logs.
#[derive(Debug, Clone, Copy)]
pub struct SearchModule;

impl Module for SearchModule {
    fn id(&self) -> &'static str {
        "search"
    }

    fn name(&self) -> &'static str {
        "Search"
    }

    fn description(&self) -> &'static str {
        "Full-text search across project files and logs via Meilisearch"
    }

    fn dependencies(&self) -> &[&'static str] {
        &["core"]
    }

    fn is_global(&self) -> bool {
        false
    }

    fn is_core(&self) -> bool {
        false
    }

    fn context_type_metadata(&self) -> Vec<TypeMeta> {
        vec![TypeMeta {
            context_type: "search_result",
            icon_id: "search",
            is_fixed: false,
            needs_cache: false,
            fixed_order: None,
            display_name: "search",
            short_name: "search",
            needs_async_wait: false,
        }]
    }

    fn dynamic_panel_types(&self) -> Vec<Kind> {
        vec![Kind::new("search_result")]
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        // Tool definitions added in Phase 6
        vec![]
    }

    fn execute_tool(&self, _tool: &ToolUse, _state: &mut State) -> Option<ToolResult> {
        // Tool execution added in Phase 6
        None
    }

    fn create_panel(&self, _context_type: &Kind) -> Option<Box<dyn Panel>> {
        // Panel creation added in Phase 6
        None
    }

    fn init_state(&self, state: &mut State) {
        let project_path = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let project_hash = hash_project_path(&project_path);

        let persist = SearchPersistData {
            port: 0,
            master_key: String::new(),
            project_hash,
            index_ready: false,
        };

        state.set_ext(SearchState { persist });
    }

    fn reset_state(&self, state: &mut State) {
        self.init_state(state);
    }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        state
            .get_ext::<SearchState>()
            .and_then(|s| serde_json::to_value(&s.persist).ok())
            .unwrap_or(serde_json::Value::Null)
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Ok(persist) = serde_json::from_value::<SearchPersistData>(data.clone()) {
            state.set_ext(SearchState { persist });
        }
    }

    fn save_worker_data(&self, _state: &State) -> serde_json::Value {
        serde_json::Value::Null
    }

    fn load_worker_data(&self, _data: &serde_json::Value, _state: &mut State) {}

    fn pre_flight(
        &self,
        _tool: &ToolUse,
        _state: &State,
    ) -> Option<cp_base::tools::pre_flight::Verdict> {
        None
    }

    fn fixed_panel_types(&self) -> Vec<Kind> {
        vec![]
    }

    fn fixed_panel_defaults(&self) -> Vec<(Kind, &'static str, bool)> {
        vec![]
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, cp_base::modules::ToolVisualizer)> {
        vec![]
    }

    fn context_display_name(&self, _context_type: &str) -> Option<&'static str> {
        None
    }

    fn context_detail(
        &self,
        _ctx: &cp_base::state::context::Entry,
    ) -> Option<String> {
        None
    }

    fn overview_context_section(&self, _state: &State) -> Option<String> {
        None
    }

    fn overview_render_sections(
        &self,
        _state: &State,
    ) -> Vec<(u8, Vec<cp_render::Block>)> {
        vec![]
    }

    fn on_close_context(
        &self,
        _ctx: &cp_base::state::context::Entry,
        _state: &mut State,
    ) -> Option<Result<String, String>> {
        None
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Search", "Full-text search via Meilisearch")]
    }

    fn on_user_message(&self, _state: &mut State) {}

    fn on_stream_stop(&self, _state: &mut State) {}

    fn on_tool_progress(
        &self,
        _tool_name: &str,
        _input_so_far: &str,
        _state: &mut State,
    ) {}

    fn on_tool_complete(&self, _tool_name: &str, _state: &mut State) {}

    fn watch_paths(&self, _state: &State) -> Vec<cp_base::panels::WatchSpec> {
        vec![]
    }

    fn should_invalidate_on_fs_change(
        &self,
        _ctx: &cp_base::state::context::Entry,
        _changed_path: &str,
        _is_dir_event: bool,
    ) -> bool {
        false
    }

    fn watcher_immediate_refresh(&self) -> bool {
        true
    }
}
