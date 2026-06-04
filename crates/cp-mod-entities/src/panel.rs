//! Fixed Entities panel — live schema, sample data, and empty-state guide.

use cp_base::panels::{ContextItem, Panel};
use cp_base::state::context::Kind;
use cp_base::state::runtime::State;
use cp_render::{Block, Semantic, Span};

use std::fmt::Write as _;

use crate::types::EntitiesState;
use crate::{db, migrations};

/// Context type identifier for entity result panels.
pub(crate) const ENTITY_RESULT_TYPE: &str = "entity_result";

/// Metadata key used to persist panel content across reloads.
const META_CONTENT: &str = "result_content";

/// Fixed panel showing entity schema + sample data.
#[derive(Debug)]
pub(crate) struct EntitiesPanel;

impl Panel for EntitiesPanel {
    fn title(&self, _state: &State) -> String {
        "Entities".to_string()
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        let es = EntitiesState::get(state);
        let content = build_context_text(es);
        let entry = state.context.iter().find(|e| e.context_type.as_str() == Kind::ENTITIES);
        let (id, last_refresh_ms) = entry.map_or_else(|| (String::new(), 0), |e| (e.id.clone(), e.last_refresh_ms));
        vec![ContextItem::new(id, "Entities", content, last_refresh_ms)]
    }

    fn blocks(&self, state: &State) -> Vec<Block> {
        let es = EntitiesState::get(state);

        if es.table_count() == 0 {
            return empty_state_blocks();
        }

        populated_blocks(es)
    }

    fn refresh(&self, state: &mut State) {
        // Re-introspect the database and update the cache.
        // Guard: don't open (and auto-create) the DB if it doesn't exist —
        // that would create an empty DB, and a subsequent save would overwrite
        // the good dump file with empty data, destroying the recovery source.
        let db_path = EntitiesState::get(state).db_path.clone();
        if !db_path.exists() {
            return;
        }

        if let Ok(conn) = db::open(&db_path) {
            let fresh = db::introspect(&conn, &db_path);
            EntitiesState::get_mut(state).schema_cache = Some(fresh);
        }

        // Update context entry
        let content = build_context_text(EntitiesState::get(state));
        let tokens = cp_base::state::context::estimate_tokens(&content);

        if let Some(ctx) = state.context.iter_mut().find(|e| e.context_type.as_str() == Kind::ENTITIES) {
            ctx.cached_content = Some(content);
            ctx.token_count = tokens;
            ctx.full_token_count = tokens;
        }
    }

    fn needs_cache(&self) -> bool {
        false
    }

    fn max_freezes(&self) -> u8 {
        2
    }

    fn handle_key(&self, _key: &crossterm::event::KeyEvent, _state: &State) -> Option<cp_base::state::actions::Action> {
        None
    }

    fn refresh_cache(&self, _request: cp_base::panels::CacheRequest) -> Option<cp_base::panels::CacheUpdate> {
        None
    }

    fn build_cache_request(
        &self,
        _ctx: &cp_base::state::context::Entry,
        _state: &State,
    ) -> Option<cp_base::panels::CacheRequest> {
        None
    }

    fn apply_cache_update(
        &self,
        _update: cp_base::panels::CacheUpdate,
        _ctx: &mut cp_base::state::context::Entry,
        _state: &mut State,
    ) -> bool {
        false
    }

    fn cache_refresh_interval_ms(&self) -> Option<u64> {
        None
    }

    fn suicide(&self, _ctx: &cp_base::state::context::Entry, _state: &State) -> bool {
        false
    }
}

// =============================================================================
// Context text (sent to LLM)
// =============================================================================

/// Build the text sent to the LLM as context for the Entities panel.
fn build_context_text(es: &EntitiesState) -> String {
    let Some(cache) = &es.schema_cache else {
        return "Entity Database (empty)\n\nNo entity tables yet. Use entity_sql to create your schema.".to_string();
    };

    if cache.tables.is_empty() {
        return "Entity Database (empty)\n\nNo entity tables yet. Use entity_sql to create your schema.".to_string();
    }

    let total_rows: u64 = cache.tables.iter().map(|t| t.row_count).sum();
    let kb = cache.db_size_bytes.wrapping_div(1024);

    let mut out = format!("Entity Database ({} tables, {} rows, {} KB):\n\n", cache.tables.len(), total_rows, kb,);

    // Open connection for sample data
    let conn = db::open(&es.db_path).ok();

    for table in &cache.tables {
        // Table header: name (row_count):
        let _header = writeln!(out, "{} ({} rows):", table.name, table.row_count);

        // Columns
        let col_desc: Vec<String> = table
            .columns
            .iter()
            .map(|c| {
                let mut s = format!("{} {}", c.name, c.col_type);
                if c.is_pk {
                    s.push_str(" PK");
                }
                s
            })
            .collect();
        let _cols = writeln!(out, "  {}", col_desc.join(", "));

        // Foreign keys
        for fk in &table.foreign_keys {
            let _fk = writeln!(out, "  FK: {} → {}({})", fk.from_col, fk.to_table, fk.to_col);
        }

        // Sample data (3 rows, 50 char truncation, skip >10 columns)
        if let Some(ref c) = conn {
            let samples = db::sample_rows(c, &table.name, 3);
            if !samples.is_empty() {
                for row in &samples {
                    let _sample = writeln!(out, "  Sample: ({})", row.join(", "));
                }
            } else if table.row_count > 0 && table.columns.len() > 10 {
                out.push_str("  (wide table, sample omitted)\n");
            } else {
                out.push_str("  (empty)\n");
            }
        }

        out.push('\n');
    }

    out
}

// =============================================================================
// Populated blocks (IR rendering)
// =============================================================================

/// Blocks for a populated database.
fn populated_blocks(es: &EntitiesState) -> Vec<Block> {
    let Some(cache) = &es.schema_cache else {
        return vec![Block::text("Entity Database (loading...)".to_string())];
    };

    let total_rows: u64 = cache.tables.iter().map(|t| t.row_count).sum();
    let kb = cache.db_size_bytes.wrapping_div(1024);

    let mut blocks = vec![
        Block::Line(vec![
            Span::styled(
                format!("Entity Database ({} tables, {} rows, {} KB)", cache.tables.len(), total_rows, kb),
                Semantic::Accent,
            )
            .bold(),
        ]),
        Block::empty(),
    ];

    // Get migration count
    if let Ok(conn) = db::open(&es.db_path) {
        let mig_count = migrations::migration_count(&conn);
        if mig_count > 0 {
            blocks.push(Block::Line(vec![Span::styled(format!("{mig_count} migration(s) tracked"), Semantic::Muted)]));
            blocks.push(Block::empty());
        }
    }

    for table in &cache.tables {
        // Table name + row count
        blocks.push(Block::Line(vec![
            Span::styled(table.name.clone(), Semantic::Accent).bold(),
            Span::new(format!(" ({} rows)", table.row_count)),
        ]));

        // Columns
        for col in &table.columns {
            let pk_marker = if col.is_pk { " PK" } else { "" };
            let nn_marker = if col.is_not_null { " NOT NULL" } else { "" };
            blocks.push(Block::Line(vec![
                Span::new(format!("  {} ", col.name)),
                Span::styled(format!("{}{pk_marker}{nn_marker}", col.col_type), Semantic::Code),
            ]));
        }

        // Foreign keys
        for fk in &table.foreign_keys {
            blocks.push(Block::Line(vec![Span::styled(
                format!("  FK: {} → {}({})", fk.from_col, fk.to_table, fk.to_col),
                Semantic::Muted,
            )]));
        }

        blocks.push(Block::empty());
    }

    blocks
}

// =============================================================================
// Empty state blocks (onboarding)
// =============================================================================

/// Blocks for the empty-state panel (onboarding guide).
fn empty_state_blocks() -> Vec<Block> {
    vec![
        Block::text("Entity Database (empty)".to_string()),
        Block::empty(),
        Block::text("No entity tables yet. Use entity_sql to create your schema.".to_string()),
        Block::empty(),
        Block::Line(vec![Span::new("Quick start:".to_string()).bold()]),
        Block::Line(vec![Span::styled(
            "  CREATE TABLE companies (id INTEGER PRIMARY KEY, name TEXT NOT NULL, country TEXT);".to_string(),
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
            "  INSERT INTO companies (name, country) VALUES ('Acme', 'France') RETURNING *;".to_string(),
            Semantic::Code,
        )]),
        Block::empty(),
        Block::Line(vec![Span::new("Tips:".to_string()).bold()]),
        Block::text("  - INTEGER PRIMARY KEY = auto-increment (don't use AUTOINCREMENT)".to_string()),
        Block::text("  - FOREIGN KEY constraints model relationships".to_string()),
        Block::text("  - SQLite types: TEXT, INTEGER, REAL, BLOB (VARCHAR(N) length is ignored)".to_string()),
        Block::text("  - Use RETURNING * on INSERT/UPDATE to see results immediately".to_string()),
        Block::text("  - For graph patterns: edges(source_id, target_id, rel_type)".to_string()),
    ]
}

// =============================================================================
// Entity result panel (dynamic, for large query results)
// =============================================================================

/// Create a dynamic entity result panel with the given content.
///
/// Returns the panel ID string (e.g., "P15").
pub(crate) fn create_result_panel(state: &mut State, title: &str, content: &str) -> String {
    use cp_base::state::context::{compute_total_pages, estimate_tokens, make_default_entry};

    let panel_id = state.next_available_context_id();
    let uid = format!("UID_{}_P", state.global_next_uid);
    state.global_next_uid = state.global_next_uid.saturating_add(1);

    let mut elem = make_default_entry(&panel_id, Kind::new(ENTITY_RESULT_TYPE), title, false);
    elem.uid = Some(uid);
    elem.cached_content = Some(content.to_string());
    elem.token_count = estimate_tokens(content);
    elem.full_token_count = elem.token_count;
    elem.total_pages = compute_total_pages(elem.token_count);
    drop(elem.metadata.insert(META_CONTENT.to_string(), serde_json::Value::String(content.to_string())));

    state.context.push(elem);
    panel_id
}

/// Panel renderer for entity SQL result panels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EntityResultPanel;

/// Cache request for restoring content from metadata after reload.
struct EntityRestoreRequest {
    /// Panel context ID (e.g., "P15").
    context_id: String,
    /// Full panel content to restore.
    content: String,
}

impl Panel for EntityResultPanel {
    fn needs_cache(&self) -> bool {
        true
    }

    fn build_cache_request(
        &self,
        ctx: &cp_base::state::context::Entry,
        _state: &State,
    ) -> Option<cp_base::panels::CacheRequest> {
        // Only need to restore if cached_content is missing (post-reload)
        if ctx.cached_content.is_some() {
            return None;
        }
        let content = ctx.metadata.get(META_CONTENT)?.as_str()?;
        Some(cp_base::panels::CacheRequest {
            context_type: Kind::new(ENTITY_RESULT_TYPE),
            data: Box::new(EntityRestoreRequest { context_id: ctx.id.clone(), content: content.to_string() }),
        })
    }

    fn apply_cache_update(
        &self,
        update: cp_base::panels::CacheUpdate,
        ctx: &mut cp_base::state::context::Entry,
        _state: &mut State,
    ) -> bool {
        use cp_base::panels::update_if_changed;
        use cp_base::state::context::{compute_total_pages, estimate_tokens};

        if let cp_base::panels::CacheUpdate::Content { content, token_count, .. } = update {
            ctx.cached_content = Some(content.clone());
            ctx.full_token_count = token_count;
            ctx.total_pages = compute_total_pages(token_count);
            ctx.current_page = 0;
            if ctx.total_pages > 1 {
                let page_content = cp_base::panels::paginate_content(
                    ctx.cached_content.as_deref().unwrap_or(""),
                    ctx.current_page,
                    ctx.total_pages,
                );
                ctx.token_count = estimate_tokens(&page_content);
            } else {
                ctx.token_count = token_count;
            }
            ctx.cache_deprecated = false;
            let _ = update_if_changed(ctx, &content);
            true
        } else {
            false
        }
    }

    fn refresh_cache(&self, request: cp_base::panels::CacheRequest) -> Option<cp_base::panels::CacheUpdate> {
        let req = request.data.downcast::<EntityRestoreRequest>().ok()?;
        let token_count = cp_base::state::context::estimate_tokens(&req.content);
        Some(cp_base::panels::CacheUpdate::Content {
            context_id: req.context_id.clone(),
            content: req.content.clone(),
            token_count,
        })
    }

    fn handle_key(&self, key: &crossterm::event::KeyEvent, _state: &State) -> Option<cp_base::state::actions::Action> {
        cp_base::panels::scroll_key_action(key)
    }

    fn blocks(&self, state: &State) -> Vec<Block> {
        let ctx = state.context.get(state.selected_context).filter(|c| c.context_type == Kind::new(ENTITY_RESULT_TYPE));

        let Some(ctx) = ctx else {
            return vec![Block::styled_text("No entity result panel".into(), Semantic::Muted)];
        };

        let Some(content) = &ctx.cached_content else {
            return vec![Block::Line(vec![Span::muted("Loading...".into()).italic()])];
        };

        content.lines().map(|line| Block::text(format!(" {line}"))).collect()
    }

    fn title(&self, state: &State) -> String {
        state.context.get(state.selected_context).map_or_else(|| "Entity Result".to_string(), |ctx| ctx.name.clone())
    }

    fn max_freezes(&self) -> u8 {
        0
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        state
            .context
            .iter()
            .filter(|c| c.context_type == Kind::new(ENTITY_RESULT_TYPE))
            .filter_map(|c| {
                let content = c.cached_content.as_ref()?;
                let output = cp_base::panels::paginate_content(content, c.current_page, c.total_pages);
                Some(ContextItem::new(&c.id, &c.name, output, c.last_refresh_ms))
            })
            .collect()
    }

    fn refresh(&self, _state: &mut State) {}

    fn cache_refresh_interval_ms(&self) -> Option<u64> {
        None
    }

    fn suicide(&self, _ctx: &cp_base::state::context::Entry, _state: &State) -> bool {
        false
    }
}
