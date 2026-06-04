# cp-mod-entities — Design Document

> **Status:** Draft v6
> **Date:** 2026-06-04  
> **Crate:** `crates/cp-mod-entities/`  
> **Depends on:** cp-base, cp-render, cp-mod-search, rusqlite

---

## 1. Vision

Give the AI a **persistent relational database** for structured domain knowledge.

The AI currently has three storage mechanisms:

| Mechanism | Structure | Queries | Updates | Relationships |
|-----------|-----------|---------|---------|---------------|
| Memories | Flat key-value | No | Yes | No |
| Scratchpad | Ephemeral cells | No | Yes | No |
| Logs | Append-only | Search only | No | No |

None support relational queries. *"Which engineers at French companies work on active projects?"* has no answer path today.

**cp-mod-entities** fills this gap: embedded SQLite, one `entity_sql` tool for arbitrary SQL, automatic Meilisearch sync for fuzzy discovery, and a fixed panel with live schema + sample data. The AI owns the schema — nothing is hard-coded.

Not every project needs entities. They shine when the AI accumulates structured knowledge that requires **cross-entity queries** — people, companies, systems, dependencies. For isolated facts, memories are simpler and sufficient.

### Why Now

- **Dependency budget recovered.** Typst removal dropped 163 packages (553 → 348). rusqlite adds ~5.
- **Meilisearch exists.** Global server, per-project indexes, background sync — all operational. Entity sync piggybacks.
- **LLMs write SQL fluently.** SQL is the natural structured interface.
- **`cc` already in tree.** rusqlite `bundled` compiles SQLite via `cc` (used by openssl-sys). Zero new build tooling.

---

## 2. Principles

1. **AI owns the schema** — no hard-coded entity types. Conventions, not constraints.
2. **SQL is the interface** — one tool, full power. LLMs are excellent at SQL.
3. **Meilisearch for discovery** — auto-indexed for fuzzy search via existing infrastructure.
4. **Single-file persistence** — SQLite at `.context-pilot/entities.db`.
5. **Zero external services** — SQLite compiles into the binary.

---

## 3. Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Storage engine | **SQLite (rusqlite, bundled)** | ACID, full SQL, in-process, 24+ years maturity. Meilisearch explicitly unsuitable as primary store (no ACID, async indexing). |
| Schema management | **No ORM** | The AI IS the schema manager. It reads the panel, writes SQL. An ORM adds a translation layer that reduces flexibility. If the AI wants schema docs, it creates a `_notes` table. |
| Meilisearch sync | **Fire-and-forget, full re-index** | Re-index all user tables after any write. Same pattern as `sync_logs_to_meilisearch`. Meilisearch down → skip silently. |
| Schema guidance | **Suggested, not enforced** | Tool description includes conventions. AI decides. |
| Sample data in panel | **Yes, capped** | First 3 rows per table in panel context. Prevents wasted "exploration SELECTs." Capped: skip tables >10 columns, truncate values at 50 chars. |
| Error enrichment | **Fuzzy suggestions** | On "table/column not found" errors, suggest closest match from schema. Include schema in all error responses. |
| Git tracking | **Gitignore** | Binary files don't belong in git. AI can recreate schema. |

**Open:** Embedder for entities index — keyword search may suffice for short entity text. Decide during Phase 3.

---

## 4. Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                        AI / LLM                               │
│                                                                │
│  entity_sql("SELECT p.name, c.name FROM people p              │
│              JOIN companies c ON p.company_id = c.id           │
│              WHERE c.country = 'France'")                      │
└───────────────────────────┬──────────────────────────────────┘
                            │ tool call
                            ▼
┌──────────────────────────────────────────────────────────────┐
│                     cp-mod-entities                            │
│                                                                │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────┐  │
│  │  tools.rs     │   │  panel.rs     │   │  sync.rs          │  │
│  │  SQL executor │   │  Schema view  │   │  Meili bridge     │  │
│  └──────┬───────┘   └──────┬───────┘   └────────┬─────────┘  │
│         │                  │                      │            │
│         ▼                  ▼                      ▼            │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  db.rs — Connection factory + PRAGMAs + bootstrap       │   │
│  └────────────────────────┬───────────────────────────────┘   │
│                           │                                    │
│                           ▼                                    │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  SQLite (WAL mode, FK ON, busy_timeout 5s)              │   │
│  │  .context-pilot/entities.db                              │   │
│  └────────────────────────────────────────────────────────┘   │
│                           │                                    │
│                    on write: fire-and-forget                    │
│                           ▼                                    │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  Meilisearch index: cp_{project_hash}_entities          │   │
│  └────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

### Crate layout

```
crates/cp-mod-entities/src/
├── lib.rs      ~200 lines   Module trait impl (mirrors cp-mod-memory/src/lib.rs)
├── db.rs       ~250 lines   Connection factory, bootstrap, introspection
├── tools.rs    ~350 lines   SQL execution, classification, formatting
├── panel.rs    ~200 lines   Fixed Entities panel
├── sync.rs     ~200 lines   Meilisearch sync (mirrors sync_logs_to_meilisearch)
└── types.rs    ~100 lines   State types
```

---

## 5. Data Model

### 5.1 SQLite

**Connection model:** Per-call (Connection is `!Send`). Open → PRAGMAs → operate → drop.

**PRAGMAs:** `journal_mode=WAL`, `foreign_keys=ON`, `busy_timeout=5000`, `journal_size_limit=64MB`.

**Bootstrap:** `_meta` table with `schema_version` and `created_at`. Everything else is AI-created.

**Introspection:** `sqlite_master` for table names, `PRAGMA table_info` for columns, `PRAGMA foreign_key_list` for FKs, `COUNT(*)` for row counts. Excludes `sqlite_%` and `_meta`.

**Integrity:** `PRAGMA integrity_check` on load. If corrupt → log warning, re-create. Self-healing, never panic.

**Checkpoint:** `PRAGMA wal_checkpoint(PASSIVE)` on save. Module returns `Value::Null` — SQLite persists itself.

### 5.2 State

```rust
pub struct EntitiesState {
    pub db_path: PathBuf,
    pub schema_cache: Option<SchemaCache>,
    pub meili_port: u16,            // 0 = unavailable
    pub meili_key: String,
    pub entities_index_uid: String, // "cp_{hash}_entities"
}

pub struct SchemaCache {
    pub tables: Vec<TableInfo>,
    pub db_size_bytes: u64,
}

pub struct TableInfo {
    pub name: String,
    pub row_count: u64,
    pub columns: Vec<ColumnInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

pub struct ColumnInfo {
    pub name: String,
    pub col_type: String,
    pub is_pk: bool,
    pub is_not_null: bool,
}

pub struct ForeignKeyInfo {
    pub from_col: String,
    pub to_table: String,
    pub to_col: String,
}
```

No `Connection` in state (`!Send`). No JSON to persist (SQLite is self-persisting). DB path: `cwd / ".context-pilot" / "entities.db"`.

### 5.3 Meilisearch

**Document format** (one per SQLite row):

```json
{
  "id": "companies__42",
  "entity_table": "companies",
  "name": "Acme Corp",
  "country": "France",
  "founded": 2019,
  "_all_text": "Acme Corp France 2019"
}
```

Primary key: `{table}__{rowid}`. `_all_text` is a space-joined concatenation of all TEXT column values.

**Index settings:**

```json
{
  "searchableAttributes": ["_all_text"],
  "filterableAttributes": ["entity_table"],
  "sortableAttributes": [],
  "typoTolerance": { "enabled": true, "minWordSizeForTypos": { "oneTypo": 4, "twoTypos": 8 } }
}
```

**Sync rules:**
- After any write → re-index ALL user tables (fire-and-forget via `MeiliClient::add_documents`).
- After `DROP TABLE` → `delete_documents_by_filter("entity_table = '{table}'")`.
- On module init → full re-index.
- Meilisearch down? Skip silently. SQL operations work independently.

---

## 6. Tool: `entity_sql`

### Definition

```yaml
entity_sql:
  description: >
    Execute SQL against the project's entity database (SQLite). The database
    is empty on first use — create your own schema. See the Entities panel
    for current schema, sample data, and getting-started tips.

    Use entities for structured data with relationships that need querying.
    Use memories for isolated facts. Use logs for events and decisions.

    Supports full SQLite: JOINs, CTEs, window functions, foreign keys,
    triggers, views. Multi-statement (semicolons) executes atomically.
    Use RETURNING * on INSERT/UPDATE to see results without a separate SELECT.
    Use CREATE TABLE IF NOT EXISTS for idempotent schema setup.
  params:
    sql:
      type: string
      required: true
```

### Execution semantics

| SQL type | Detection | Return value | Triggers sync? |
|----------|-----------|-------------|----------------|
| SELECT / EXPLAIN / PRAGMA | Trimmed uppercase starts with keyword | Markdown table (≤50 rows inline, >50 → `entity_result` panel) | No |
| INSERT / UPDATE / DELETE | Starts with DML keyword | `"N row(s) affected."` | Yes |
| CREATE / ALTER / DROP / CREATE INDEX | Starts with DDL keyword | Full schema summary | Yes |
| WITH ... SELECT (CTE) | Starts with WITH, no DML keywords | Markdown table | No |
| WITH ... INSERT/UPDATE/DELETE | Starts with WITH, contains DML | Affected rows | Yes |
| Error | SQLite returns error | `is_error: true` + enriched error (see below) | No |

**Conservative fallback:** if classification is ambiguous, treat as write (sync is idempotent).

### Error enrichment

Raw SQLite errors are wrapped with context for self-correction: fuzzy-match suggestions on unknown table/column names (Levenshtein ≤2), constraint details on violations, and the current schema summary appended to every error.

### Multi-statement handling

Split on `;` respecting single-quoted string literals (state machine tracking `in_string`, handling `''` escapes). All statements execute within a single implicit transaction — any error rolls back the entire batch. Return the result of the last statement.

### Result format

```
| col1 | col2 | col3 |
|------|------|------|
| val  | val  | val  |

(N rows)
```

NULL → `NULL`. BLOB → `[BLOB N bytes]`. No alignment padding.

**Empty results:** `"0 rows returned. (Table 'X' has Y total rows.)"` — tells the AI the table isn't empty, just the filter matched nothing. Prevents unnecessary follow-up SELECTs.

**INSERT/UPDATE with RETURNING:** If the SQL includes a `RETURNING` clause, format the returned rows as a table (same as SELECT). This is the preferred pattern — the tool description recommends it.

### Lifecycle

Every `entity_sql` call: open connection → classify → execute → format result → refresh panel (`touch_panel(Kind::ENTITIES)`) → if write: fire-and-forget Meilisearch sync → drop connection.

Instrumented with `flame!("entity_sql")`.

---

## 7. Panel: Entities

Fixed panel. `Kind::ENTITIES`, `fixed_order = Some(5)` (after Memories), `needs_cache = false`.

### Populated state

Every user table (excluding `_meta`, `sqlite_%`) with name, row count, columns (name, type, PK), foreign keys. Footer: totals, DB size.

**LLM context** — schema + sample data for quick orientation:

```
Entity Database (3 tables, 89 rows, 48 KB):

companies (23 rows):
  id INTEGER PK, name TEXT, country TEXT, founded INTEGER
  FK: ceo_id → people(id)
  Sample: (1, 'Acme Corp', 'France', 2019), (2, 'Globex', 'US', 2015), (3, 'Initech', 'UK', 2021)

people (45 rows):
  id INTEGER PK, name TEXT, role TEXT, company_id INTEGER
  FK: company_id → companies(id)
  Sample: (1, 'John Doe', 'CTO', 1), (2, 'Jane Smith', 'Engineer', 2), (3, 'Bob Lee', 'PM', 1)

projects (21 rows):
  id INTEGER PK, name TEXT, status TEXT, company_id INTEGER, lead_id INTEGER
  FK: company_id → companies(id), lead_id → people(id)
  Sample: (1, 'Phoenix', 'active', 1, 1), (2, 'Atlas', 'planning', 2, 2)
```

Sample data: first 3 rows per table, values truncated at 50 chars, skip for tables >10 columns, `(empty)` for empty tables.

### Empty state (smart — carries the usage guidance)

When the database has no user tables, the panel becomes the AI's onboarding guide. This keeps the tool description lean (~14 lines) while providing rich guidance exactly when needed:

```
Entity Database (empty)

No entity tables yet. Use entity_sql to create your schema.

Quick start:
  CREATE TABLE companies (id INTEGER PRIMARY KEY, name TEXT NOT NULL, country TEXT);
  CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, role TEXT,
    company_id INTEGER REFERENCES companies(id));
  INSERT INTO companies (name, country) VALUES ('Acme', 'France') RETURNING *;

Tips:
  - INTEGER PRIMARY KEY = auto-increment (don't use AUTOINCREMENT)
  - FOREIGN KEY constraints model relationships
  - SQLite types: TEXT, INTEGER, REAL, BLOB (VARCHAR(N) length is ignored)
  - Use RETURNING * on INSERT/UPDATE to see results immediately
  - For graph patterns: edges(source_id, target_id, rel_type)
```

**IR blocks:** `Block::KeyValue` for table headers, `Block::Line` for columns. Table names → `Accent`, types → `Code`, FKs → `Muted`.

---

## 8. Module Integration

### Cargo.toml

```toml
[dependencies]
cp-base = { path = "../cp-base" }
cp-render = { path = "../cp-render" }
cp-mod-search = { path = "../cp-mod-search" }
rusqlite = { workspace = true, features = ["bundled", "column_decltype"] }
serde_json = { workspace = true }
crossterm = { workspace = true }
log = { workspace = true }
```

`rusqlite` declared as workspace dep in root Cargo.toml. `bundled` compiles SQLite via `cc`. `column_decltype` for type introspection.

### Registration

Follow cp-mod-memory pattern. Key specifics: `Kind::ENTITIES`, `fixed_order=5`, `id="entities"`, `dependencies=["search"]`, `is_global=true`, `is_core=false`. Tool category: `("Entity", "Persistent relational entity database")`. Overview: `"Entities: N tables, M rows\n"` or `None`. YAML validation count 19→20.

### Cross-Module Concerns

**MeiliClient:** Add `pub fn meili_client(state: &State) -> Option<MeiliClient>` to cp-mod-search. Currently `pub(crate)`.

**Search scope:** cp-mod-entities exposes `pub fn entities_index_uid(state: &State) -> Option<String>`. Search module calls this when scope includes entities. `None` → silently skipped. Adds `scope="entities"` to search tool.

**Visualizer:** Table headers → `Accent`, row counts → `Success`, NULLs → `Muted + dimmed`, schema → `Code`.

---

## 9. Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| SQLite C compilation fails on cross-compilation | High | `cc` already cross-compiles OpenSSL in CI. SQLite amalgamation is simpler. Test early in Phase 1. |
| rusqlite exceeds dep budget (>8 new crates) | Medium | Audit `cargo tree -p rusqlite --depth 1` before merging. |

---

## 10. Implementation Plan

### Phase 1: Crate scaffold
- [ ] Create `crates/cp-mod-entities/` with Cargo.toml + empty lib.rs
- [ ] Add to workspace members, add `rusqlite` workspace dependency
- [ ] Audit transitive deps: `cargo tree -p rusqlite --features bundled --depth 2`
- [ ] Verify compilation on all CI targets

### Phase 2: Core (DB + Tool + Panel)
- [ ] `types.rs` — EntitiesState, SchemaCache, TableInfo, ColumnInfo, ForeignKeyInfo
- [ ] `db.rs` — open_connection (PRAGMAs + bootstrap), introspect_schema, integrity_check
- [ ] `tools.rs` — SQL classification, multi-statement splitting, execution, result formatting
- [ ] `panel.rs` — blocks(), context(), refresh(), empty state
- [ ] `lib.rs` — Module trait impl (init, save/load, tool defs, panel)
- [ ] `yamls/tools/entities.yaml`
- [ ] Register in mod.rs, add Kind::ENTITIES, update YAML validation count
- [ ] All 6 callbacks green ✓

### Phase 3: Meilisearch + search scope + polish
- [ ] `sync.rs` — table → documents, upsert, delete-by-filter
- [ ] Expose `pub fn meili_client()` from cp-mod-search
- [ ] Create entities index on module init, full re-index
- [ ] Wire sync into tool execution (after writes)
- [ ] Expose `pub fn entities_index_uid()`, add `"entities"` scope to search tool
- [ ] Tool visualizer, overview context section
- [ ] Documentation

---

## 11. Future Extensions

| Extension | When |
|-----------|------|
| Graph visualization in panel (ASCII/IR) | User demand |
| Export/import (SQL dump, CSV, JSON) | Data portability needs |
| Spine notifications on entity changes | Automation use cases |
| FTS5 full-text search columns | Large text fields |

---

## Appendix A: Reference Implementations

| Pattern | Source file |
|---------|-----------|
| Module trait impl | `crates/cp-mod-memory/src/lib.rs` |
| Tool execution + `flame!()` | `crates/cp-mod-memory/src/tools.rs` |
| Fixed panel (blocks, context, refresh) | `crates/cp-mod-memory/src/panel.rs` |
| Meilisearch index creation + settings | `crates/cp-mod-search/src/meili/bootstrap.rs` |
| Fire-and-forget document upsert | `crates/cp-mod-search/src/lib.rs::sync_logs_to_meilisearch` |
| MeiliClient API | `crates/cp-mod-search/src/meili/client.rs` |
| Search tool scope handling | `crates/cp-mod-search/src/tools.rs` |
| Module registration | `src/modules/mod.rs::all_modules()` |
| Kind constant | `cp-base/src/state/context.rs` |
| Tool YAML format | `yamls/tools/memory.yaml` |
| YAML validation test | `cp-base/src/lib.rs` (compile-time, 19 → 20 files) |

### Appendix B: Dependency Audit (pre-merge)

```bash
cargo tree -p rusqlite --features bundled --depth 2 --no-default-features
```

Expected: rusqlite → libsqlite3-sys → cc (already in tree) + hashlink, fallible-iterator, fallible-streaming-iterator. Total ≤ 8 new crates.
