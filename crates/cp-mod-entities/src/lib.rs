//! Entities module — persistent relational database for structured domain knowledge.
//!
//! One tool: `entity_sql` for arbitrary SQL against an embedded SQLite database.
//! The AI owns the schema — nothing is hard-coded. Automatic Meilisearch sync
//! for fuzzy discovery. Fixed panel with live schema + sample data.

/// State types: `EntitiesState`, `SchemaCache`, `TableInfo`, `ColumnInfo`, `ForeignKeyInfo`.
pub mod types;

use types::EntitiesState;

use cp_base::modules::Module;
use cp_base::panels::Panel;
use cp_base::state::context::Kind;
use cp_base::state::runtime::State;
use cp_base::tools::pre_flight::Verdict;
use cp_base::tools::{ParamType, ToolDefinition, ToolTexts};
use cp_base::tools::{ToolResult, ToolUse};

/// Lazily parsed tool descriptions from the entities YAML definition.
static TOOL_TEXTS: std::sync::LazyLock<ToolTexts> =
    std::sync::LazyLock::new(|| ToolTexts::parse(include_str!("../../../yamls/tools/entities.yaml")));

/// Entities module: persistent relational entity database.
#[derive(Debug, Clone, Copy)]
pub struct EntitiesModule;

impl Module for EntitiesModule {
    fn id(&self) -> &'static str {
        "entities"
    }
    fn name(&self) -> &'static str {
        "Entities"
    }
    fn description(&self) -> &'static str {
        "Persistent relational entity database"
    }
    fn is_global(&self) -> bool {
        true
    }

    fn init_state(&self, state: &mut State) {
        let cwd = std::env::current_dir().unwrap_or_default();
        let cp_dir = cwd.join(".context-pilot");
        let db_path = cp_dir.join("entities.db");
        let shared_dir = cp_dir.join("shared").join("entities");
        let dump_path = shared_dir.join("schema.sql");
        let migrations_dir = shared_dir.join("migrations");

        // Ensure directories exist
        let _r = std::fs::create_dir_all(&shared_dir);
        let _r = std::fs::create_dir_all(&migrations_dir);

        state.set_ext(EntitiesState::new(db_path, dump_path, migrations_dir));
    }

    fn reset_state(&self, state: &mut State) {
        self.init_state(state);
    }

    fn save_module_data(&self, _state: &State) -> serde_json::Value {
        // SQLite is self-persisting. Full dump on save will be added in Phase 2.
        serde_json::Value::Null
    }

    fn load_module_data(&self, _data: &serde_json::Value, state: &mut State) {
        // Re-initialize state (idempotent). Recovery logic added in Phase 2.
        self.init_state(state);
    }

    fn fixed_panel_types(&self) -> Vec<Kind> {
        vec![Kind::new(Kind::ENTITIES)]
    }

    fn fixed_panel_defaults(&self) -> Vec<(Kind, &'static str, bool)> {
        vec![(Kind::new(Kind::ENTITIES), "Entities", false)]
    }

    fn create_panel(&self, context_type: &Kind) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            Kind::ENTITIES => Some(Box::new(EntitiesPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let t = &*TOOL_TEXTS;
        vec![ToolDefinition::from_yaml("entity_sql", t)
            .short_desc("Execute SQL on entity database")
            .category("Entity")
            .param("sql", ParamType::String, true)
            .build()]
    }

    fn pre_flight(&self, _tool: &ToolUse, _state: &State) -> Option<Verdict> {
        None
    }

    fn execute_tool(&self, tool: &ToolUse, _state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "entity_sql" => Some(ToolResult {
                tool_use_id: tool.id.clone(),
                content: "entity_sql not yet implemented (Phase 2)".to_string(),
                display: None,
                tldr: None,
                is_error: true,
                preserves_tempo: false,
                tool_name: tool.name.clone(),
            }),
            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, cp_base::modules::ToolVisualizer)> {
        vec![]
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::context::TypeMeta> {
        vec![cp_base::state::context::TypeMeta {
            context_type: "entities",
            icon_id: "entities",
            is_fixed: true,
            needs_cache: false,
            fixed_order: Some(5),
            display_name: "entities",
            short_name: "entities",
            needs_async_wait: false,
        }]
    }

    fn overview_context_section(&self, state: &State) -> Option<String> {
        let es = EntitiesState::get(state);
        let tc = es.table_count();
        if tc == 0 {
            return None;
        }
        Some(format!("Entities: {} tables, {} rows\n", tc, es.total_rows()))
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Entity", "Persistent relational entity database")]
    }

    fn dependencies(&self) -> &[&'static str] {
        &["search"]
    }

    fn is_core(&self) -> bool {
        false
    }

    fn save_worker_data(&self, _state: &State) -> serde_json::Value {
        serde_json::Value::Null
    }

    fn load_worker_data(&self, _data: &serde_json::Value, _state: &mut State) {}

    fn dynamic_panel_types(&self) -> Vec<Kind> {
        vec![]
    }

    fn context_display_name(&self, _context_type: &str) -> Option<&'static str> {
        None
    }

    fn context_detail(&self, _ctx: &cp_base::state::context::Entry) -> Option<String> {
        None
    }

    fn overview_render_sections(&self, _state: &State) -> Vec<(u8, Vec<cp_render::Block>)> {
        vec![]
    }

    fn on_close_context(
        &self,
        _ctx: &cp_base::state::context::Entry,
        _state: &mut State,
    ) -> Option<Result<String, String>> {
        None
    }

    fn on_user_message(&self, _state: &mut State) {}

    fn on_stream_stop(&self, _state: &mut State) {}

    fn on_tool_progress(&self, _tool_name: &str, _input_so_far: &str, _state: &mut State) {}

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

// =============================================================================
// Stub panel — will be replaced in Phase 2
// =============================================================================

/// Minimal entities panel (stub for Phase 1).
#[derive(Debug)]
struct EntitiesPanel;

impl Panel for EntitiesPanel {
    fn title(&self, _state: &State) -> String {
        "Entities".to_string()
    }

    fn context(&self, state: &State) -> Vec<cp_base::panels::ContextItem> {
        let es = EntitiesState::get(state);
        let content = if es.table_count() == 0 {
            "Entity Database (empty)\n\nNo entity tables yet. Use entity_sql to create your schema."
                .to_string()
        } else {
            format!(
                "Entity Database ({} tables, {} rows)",
                es.table_count(),
                es.total_rows()
            )
        };
        let entry = state.context.iter().find(|e| e.context_type.as_str() == Kind::ENTITIES);
        let (id, last_refresh_ms) = entry.map_or_else(
            || (String::new(), 0),
            |e| (e.id.clone(), e.last_refresh_ms),
        );
        vec![cp_base::panels::ContextItem::new(id, "Entities", content, last_refresh_ms)]
    }

    fn blocks(&self, state: &State) -> Vec<cp_render::Block> {
        let es = EntitiesState::get(state);

        if es.table_count() == 0 {
            return empty_state_blocks();
        }

        vec![cp_render::Block::text(format!(
            "Entity Database ({} tables, {} rows)",
            es.table_count(),
            es.total_rows()
        ))]
    }

    fn refresh(&self, state: &mut State) {
        let es = EntitiesState::get(state);
        let content = if es.table_count() == 0 {
            "Entity Database (empty)\n\nNo entity tables yet. Use entity_sql to create your schema."
                .to_string()
        } else {
            format!(
                "Entity Database ({} tables, {} rows)",
                es.table_count(),
                es.total_rows()
            )
        };
        let tokens = cp_base::state::context::estimate_tokens(&content);
        if let Some(ctx) = state
            .context
            .iter_mut()
            .find(|e| e.context_type.as_str() == Kind::ENTITIES)
        {
            ctx.cached_content = Some(content);
            ctx.token_count = tokens;
            ctx.full_token_count = tokens;
        }
    }

    fn needs_cache(&self) -> bool {
        false
    }

    fn max_freezes(&self) -> u8 {
        0
    }
}

/// Blocks for the empty-state panel (onboarding guide).
fn empty_state_blocks() -> Vec<cp_render::Block> {
    use cp_render::{Block, Semantic, Span};

    vec![
        Block::text("Entity Database (empty)".to_string()),
        Block::empty(),
        Block::text(
            "No entity tables yet. Use entity_sql to create your schema.".to_string(),
        ),
        Block::empty(),
        Block::Line(vec![Span::new("Quick start:".to_string()).bold()]),
        Block::Line(vec![Span::styled(
            "  CREATE TABLE companies (id INTEGER PRIMARY KEY, name TEXT NOT NULL, country TEXT);"
                .to_string(),
            Semantic::Code,
        )]),
        Block::Line(vec![Span::styled(
            "  CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, role TEXT,".to_string(),
            Semantic::Code,
        )]),
        Block::Line(vec![Span::styled(
            "    company_id INTEGER REFERENCES companies(id));".to_string(),
            Semantic::Code,
        )]),
        Block::Line(vec![Span::styled(
            "  INSERT INTO companies (name, country) VALUES ('Acme', 'France') RETURNING *;"
                .to_string(),
            Semantic::Code,
        )]),
        Block::empty(),
        Block::Line(vec![Span::new("Tips:".to_string()).bold()]),
        Block::text(
            "  - INTEGER PRIMARY KEY = auto-increment (don't use AUTOINCREMENT)".to_string(),
        ),
        Block::text(
            "  - FOREIGN KEY constraints model relationships".to_string(),
        ),
        Block::text(
            "  - SQLite types: TEXT, INTEGER, REAL, BLOB (VARCHAR(N) length is ignored)"
                .to_string(),
        ),
        Block::text(
            "  - Use RETURNING * on INSERT/UPDATE to see results immediately".to_string(),
        ),
        Block::text(
            "  - For graph patterns: edges(source_id, target_id, rel_type)".to_string(),
        ),
    ]
}
