# Design Document: `cp-mod-entities`
## AI-Managed Relational Entity Store

| Field | Value |
|-------|-------|
| **Status** | Draft v2 — under review |
| **Date** | 2026-06-04 |
| **Author** | Guillaume (with AI assistance) |
| **Reviewers** | — |
| **Crate** | `crates/cp-mod-entities/` |
| **Dependencies** | cp-base, cp-render, cp-mod-search, rusqlite |

---

## 1. Executive Summary

This document specifies `cp-mod-entities`, a new Context Pilot module that gives the AI a **persistent relational database** for storing and querying structured domain knowledge — people, companies, projects, relationships, or any schema the AI designs.

The module embeds **SQLite** (via `rusqlite` with the `bundled` feature) directly into the Context Pilot binary, exposing a single `entity_sql` tool that accepts arbitrary SQL. Entity data is automatically synchronized to **Meilisearch** for fuzzy search and discovery, and a fixed **Entities panel** provides the AI with continuous schema awareness.

**Key design choice:** The AI has full schema freedom. No entity types, columns, or relationships are hard-coded. The module provides the engine; the AI provides the schema.

---

## 2. Background & Motivation

### 2.1 Problem

The AI currently stores knowledge through three mechanisms, none of which support structured relationships:

| Mechanism | Limitations |
|-----------|------------|
| **Memories** (`cp-mod-memory`) | Flat key-value. No relations, no queries, no joins. Each memory is an isolated fact with a `tl_dr`, optional `contents`, and labels. |
| **Scratchpad** (`cp-mod-scratchpad`) | Ephemeral cells. No persistence guarantees across sessions (per-worker). No query capability. |
| **Logs** (`cp-mod-logs`) | Append-only. No updates, no deletions, no structure. Designed for event recording, not knowledge management. |

When the AI needs to answer *"Which engineers at French companies are working on projects with status 'active'?"*, it must scan memories, grep through files, or ask the user. There is no relational query path.

### 2.2 Why Now

- **Dependency budget recovered:** Removing `cp-mod-typst` dropped 163 transitive packages (553 → 348). Adding `rusqlite` (~5 packages) is a negligible cost.
- **Meilisearch infrastructure exists:** The search module already manages a global embedded Meilisearch server with per-project indexes, background indexing, and semantic search. Entity sync piggybacks on this.
- **LLM SQL fluency is proven:** Modern LLMs produce correct SQLite queries with extremely high reliability. SQL is the most natural structured interface for an AI.
- **`cc` crate already in tree:** `openssl-sys` (vendored) uses `cc` for C compilation. `rusqlite`'s `bundled` feature uses the same mechanism — zero new build tooling.

### 2.3 Design Principles

1. **AI has full schema freedom** — no hard-coded entity types. We provide conventions, not constraints.
2. **SQL is the interface** — one tool, maximum power. LLMs are excellent at SQL.
3. **Meilisearch for discovery** — entity data auto-indexed for fuzzy search, leveraging existing infrastructure.
4. **Single-file persistence** — SQLite at `.context-pilot/entities.db`, consistent with the project's state model.
5. **Zero external services** — SQLite compiles into the binary. No server, no config, no ports.
6. **Follow existing patterns** — implementation mirrors `cp-mod-memory` (state/tools/panel structure) and `cp-mod-search` (Meilisearch integration).

---

## 3. Scope

### 3.1 In Scope (v1)

- SQLite database lifecycle (create, open, configure, checkpoint)
- `entity_sql` tool for arbitrary SQL execution
- Fixed Entities panel with schema introspection
- Meilisearch sync for entity discovery
- Integration with existing `search` tool (new `entities` scope)
- Module registration, save/load, activate/deactivate

### 3.2 Out of Scope (v1)

- Graph visualization in the panel
- Schema migration tracking
- Cross-project/global entity databases
- SQL dump export/import tools
- Spine notifications on entity changes
- SQLite virtual tables

---

## 4. Architecture Overview

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
│  │  SQLite (rusqlite, WAL mode, FK ON)                     │   │
│  │  .context-pilot/entities.db                              │   │
│  └────────────────────────────────────────────────────────┘   │
│                           │                                    │
│                    on write: fire-and-forget                    │
│                           ▼                                    │
│  ┌────────────────────────────────────────────────────────┐   │
│  │  Meilisearch index: cp_{project_hash}_entities          │   │
│  │  (fuzzy search, filters, facets via cp-mod-search)      │   │
│  └────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

### 4.1 Component Responsibilities

| Component | Responsibility | Reference Pattern |
|-----------|---------------|-------------------|
| `lib.rs` | `Module` trait impl — init, save/load, tool defs, panel creation | `cp-mod-memory/src/lib.rs` |
| `db.rs` | Connection factory, PRAGMA setup, bootstrap, schema introspection | New (no existing equivalent) |
| `tools.rs` | `entity_sql` execution, SQL classification, result formatting | `cp-mod-memory/src/tools.rs` |
| `panel.rs` | Fixed Entities panel — blocks, context, refresh | `cp-mod-memory/src/panel.rs` |
| `sync.rs` | SQLite → Meilisearch row synchronization | `cp-mod-search/src/lib.rs::sync_logs_to_meilisearch` |
| `types.rs` | `EntitiesState`, `SchemaInfo`, result types | `cp-mod-memory/src/types.rs` |

---

## 5. Functional Requirements

### FR-1: Database Lifecycle Management

**The module SHALL manage a per-project SQLite database file.**

| ID | Requirement | Acceptance Criteria |
|----|------------|-------------------|
| FR-1.1 | On `load_module_data()`, open or create the database at `.context-pilot/entities.db`. | File exists after first module init. |
| FR-1.2 | Set PRAGMAs on every connection: `journal_mode=WAL`, `foreign_keys=ON`, `busy_timeout=5000`, `journal_size_limit=67108864`. | `PRAGMA journal_mode` returns `wal`. `PRAGMA foreign_keys` returns `1`. |
| FR-1.3 | Bootstrap a `_meta` table with `schema_version` and `created_at` keys on first creation. | `SELECT * FROM _meta` returns two rows on fresh DB. |
| FR-1.4 | On `save_module_data()`, execute `PRAGMA wal_checkpoint(PASSIVE)` to flush WAL to main DB file. Return `serde_json::Value::Null` (no JSON state needed — SQLite persists itself). | WAL file is flushed. Module data is Null. |
| FR-1.5 | On `load_module_data()`, run `PRAGMA integrity_check` on the existing DB. If corrupt, log a warning and re-create the database from scratch. | Corrupt DB is replaced, not panicked on. |

**Implementation notes:**
- Connection handling follows the `MeiliClient` pattern from `cp-mod-search`: **no persistent connection in state**. Each tool call and panel refresh opens a fresh `rusqlite::Connection`, configures PRAGMAs, operates, and drops it. This avoids `Send`/`Sync` issues since `rusqlite::Connection` is `!Send`.
- `EntitiesState` stores only `db_path: PathBuf` and an optional `SchemaCache` (see FR-3). No `Connection` in the TypeMap.
- The DB path is resolved as: `std::env::current_dir() / ".context-pilot" / "entities.db"`.

### FR-2: SQL Execution Tool (`entity_sql`)

**The module SHALL expose a single tool `entity_sql` that accepts arbitrary SQL and returns formatted results.**

| ID | Requirement | Acceptance Criteria |
|----|------------|-------------------|
| FR-2.1 | Accept a required `sql` parameter of type `String`. | Tool schema has one required string param. |
| FR-2.2 | `SELECT` / `EXPLAIN` / `PRAGMA` queries return a markdown-formatted table. | Output contains `\|` column separators and a header row. |
| FR-2.3 | `INSERT` / `UPDATE` / `DELETE` queries return the number of affected rows (via `conn.changes()`). | Output: `"N row(s) affected."` |
| FR-2.4 | `CREATE TABLE` / `ALTER TABLE` / `DROP TABLE` / `CREATE INDEX` DDL queries return the updated full schema summary (reuse panel schema introspection from FR-3). | Output includes all current tables with columns. |
| FR-2.5 | Multi-statement SQL (separated by `;`) executes all statements within a single implicit transaction. On any error, the entire batch rolls back. Return the result of the last statement. | `"CREATE TABLE ...; INSERT ...; SELECT ..."` creates table, inserts, and returns SELECT result atomically. |
| FR-2.6 | On SQLite error, return `is_error: true` with the SQLite error message and the failing SQL fragment. | `ToolResult.is_error == true`. Content includes error context. |
| FR-2.7 | SELECT results with ≤ 50 rows are returned inline in the tool result. Results with > 50 rows create a dynamic `entity_result` panel with pagination. | Small queries: inline. Large queries: panel created with `DYN_PANEL_ID_PLACEHOLDER`. |
| FR-2.8 | After any write operation (INSERT/UPDATE/DELETE/DDL), trigger Meilisearch sync (FR-4). | Sync function called. Meilisearch index updated asynchronously. |
| FR-2.9 | After every execution (read or write), call `state.touch_panel(Kind::ENTITIES)` to refresh the panel. | Panel `last_refresh_ms` updated. |
| FR-2.10 | Instrument with `flame!("entity_sql")` for profiling. | Span appears in flame graph. |

**SQL classification logic:**
```
Read-only:  SQL trimmed+uppercased starts with SELECT, EXPLAIN, PRAGMA, WITH (followed by SELECT)
Write:      Everything else (INSERT, UPDATE, DELETE, CREATE, ALTER, DROP, REPLACE, UPSERT)
```
For `WITH ... SELECT` (CTEs), classify as read-only. For `WITH ... INSERT/UPDATE/DELETE` (CTE DML), classify as write. Conservative fallback: if unsure, treat as write (triggers sync, which is idempotent).

**Multi-statement parsing:**
Split on `;` but respect string literals (single-quoted `'...'`). Use a simple state machine: track whether we are inside a string literal, handle `''` escapes. This avoids mishandling `INSERT INTO t VALUES ('a;b')`.

**Result table formatting:**
```
| col1 | col2 | col3 |
|------|------|------|
| val  | val  | val  |

(N rows)
```
Column widths are NOT padded to alignment (the AI reads content, not visual tables). Values are stringified via SQLite's `get_ref()` → `ValueRef` → display. NULL renders as `NULL`. BLOB renders as `[BLOB N bytes]`.

**Tool definition (YAML at `yamls/tools/entities.yaml`):**
```yaml
entity_sql:
  description: >
    Execute SQL against the project's entity database (SQLite). Use for
    creating tables, inserting/updating/deleting entities, and querying
    relationships. The database is empty on first use — create your own
    schema as needed.

    Supports full SQLite: JOINs, CTEs, window functions, json_extract(),
    foreign keys, triggers, views, transactions.

    Schema conventions (suggested, not enforced):
    - Use INTEGER PRIMARY KEY for auto-increment IDs
    - Use FOREIGN KEY constraints to model relationships
    - Use TEXT for strings, INTEGER for numbers/booleans, REAL for floats
    - For graph patterns, consider a generic edges(source_type, source_id,
      target_type, target_id, rel_type) table

    Multi-statement: separate with semicolons. Executed atomically.
    Read queries return formatted tables. Writes return affected row count.
    DDL returns the updated schema overview.
```

**Tool definition (Rust builder, in `lib.rs`):**
```rust
ToolDefinition::from_yaml("entity_sql", t)
    .short_desc("Execute SQL against the entity database")
    .category("Entity")
    .param("sql", ParamType::String, true)
    .build()
```

### FR-3: Entity Overview Panel

**The module SHALL provide a fixed panel that displays the current database schema and statistics.**

| ID | Requirement | Acceptance Criteria |
|----|------------|-------------------|
| FR-3.1 | Register a fixed panel of type `Kind::ENTITIES` with `fixed_order` placing it after Memories (order 4) and before Logs. Suggested: `fixed_order = Some(5)`. | Panel appears in sidebar between Memories and existing order-5+ panels. |
| FR-3.2 | Panel title: `"Entities"`. | `panel.title()` returns `"Entities"`. |
| FR-3.3 | Panel content lists every user table (excluding `_meta` and `sqlite_%` system tables) with: table name, row count, column definitions (name, type, PK flag), and detected foreign key relationships. | Content matches schema. FK arrows rendered as `FK→table(col)`. |
| FR-3.4 | Panel footer shows: total table count, total row count, DB file size (bytes → human-readable KB/MB), WAL mode status, FK status. | Footer line present. |
| FR-3.5 | Panel `context()` returns a compact text representation of the full schema for LLM injection. Format: one line per table. | Context text includes every table and column. |
| FR-3.6 | `needs_cache() → false`. Schema introspection is fast enough to run inline. | No cache machinery needed. |
| FR-3.7 | If the database is empty (no user tables), display `"No entity tables. Use entity_sql to create your schema."`. | Empty state handled gracefully. |

**Schema introspection queries (in `db.rs`):**
```sql
-- List user tables
SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_meta' ORDER BY name;

-- Column info per table
PRAGMA table_info({table_name});
-- Returns: cid, name, type, notnull, dflt_value, pk

-- Foreign key info per table
PRAGMA foreign_key_list({table_name});
-- Returns: id, seq, table, from, to, on_update, on_delete, match

-- Row count per table
SELECT COUNT(*) FROM {table_name};
```

**LLM context format:**
```
Entity Database (3 tables, 89 rows, 48 KB):
- companies (23 rows): id INTEGER PK, name TEXT, country TEXT, founded INTEGER, ceo_id INTEGER FK→people(id)
- people (45 rows): id INTEGER PK, name TEXT, role TEXT, company_id INTEGER FK→companies(id), email TEXT
- projects (21 rows): id INTEGER PK, name TEXT, status TEXT, company_id INTEGER FK→companies(id), lead_id INTEGER FK→people(id)
```

**Panel blocks (IR rendering):**
Uses `Block::KeyValue` for table headers and `Block::Line` for column details, following the pattern in `cp-mod-memory/src/panel.rs`. Semantic colors: table names in `Semantic::Accent`, column types in `Semantic::Code`, FK references in `Semantic::Muted`.

### FR-4: Meilisearch Synchronization

**After every write operation, the module SHALL synchronize affected entity data to a dedicated Meilisearch index for discovery.**

| ID | Requirement | Acceptance Criteria |
|----|------------|-------------------|
| FR-4.1 | On module init (`load_module_data`), create the Meilisearch index `cp_{project_hash}_entities` if it does not exist. Use the project hash from `SearchState.persist.project_hash` (obtained via `cp_mod_search::meili::bootstrap::hash_project_path`). | Index exists in Meilisearch after init. |
| FR-4.2 | Index settings: `searchableAttributes: ["_all_text"]`, `filterableAttributes: ["entity_table"]`, `sortableAttributes: []`. | Settings applied. |
| FR-4.3 | After a write operation, for **each user table**: SELECT all rows, convert to Meilisearch documents, upsert via `MeiliClient::add_documents`. | Documents appear in the index. |
| FR-4.4 | Document format: primary key `"{table}__{rowid}"`, field `entity_table` set to the table name, all column values as fields, `_all_text` as a space-joined concatenation of all TEXT column values. | Document shape matches spec. |
| FR-4.5 | After a `DROP TABLE`, remove all documents with `entity_table = '{dropped_table}'` via `MeiliClient::delete_documents_by_filter`. | Documents removed from index. |
| FR-4.6 | On module init, perform a full re-index of all user tables. | Fresh start or external DB modification is handled. |
| FR-4.7 | Sync is **fire-and-forget** — `MeiliClient::add_documents` returns a task UID; the module does NOT wait for it to complete. This follows the pattern in `cp-mod-search/src/lib.rs::sync_logs_to_meilisearch`. | Tool execution is not blocked by Meilisearch indexing lag. |
| FR-4.8 | If the Meilisearch server is unavailable (port 0), skip sync silently. Entities still work — just without search discovery. | No crash on missing Meilisearch. |

**Implementation notes:**
- Accessing `SearchState` from `EntitiesState` requires reading both from the `State` TypeMap. This is safe since we read `SearchState` immutably for port/key, then release it before any other TypeMap access.
- The sync function opens its own `rusqlite::Connection` to read rows, separate from the tool execution connection (which is already dropped by then).
- For v1, always re-index ALL user tables after any write (not just the affected table). User tables are small (entity data, not big data). Optimize later if profiling shows this is a bottleneck.

### FR-5: Search Integration

**Entity data SHALL be discoverable via the existing `search` tool.**

| ID | Requirement | Acceptance Criteria |
|----|------------|-------------------|
| FR-5.1 | Add `"entities"` as a valid value for the `scope` parameter of the `search` tool (in `cp-mod-search`). | `search(query="...", scope="entities")` queries the entities index. |
| FR-5.2 | When `scope="all"`, include entity results alongside file and log results. | Entities appear in `scope="all"` search results. |
| FR-5.3 | Entity search results are tagged with `entity_table` in the output so the AI knows which table they came from. | Result includes `[entity: companies]` or similar prefix. |

**Implementation notes — cross-module concern:**
The `search` tool lives in `cp-mod-search/src/tools.rs`. The entities index UID is `cp_{project_hash}_entities`. Two integration approaches:

- **Option A (recommended for v1):** `cp-mod-entities` exposes a `pub fn entities_index_uid(state: &State) -> Option<String>` function. The search module calls it when `scope` includes entities. If `None` (module not active or no DB), the scope is silently skipped. This is analogous to how `cp-mod-search/src/lib.rs` exposes `pub fn overlay_info(state: &State)` for the TUI to call.
- **Option B (future):** A generic "additional index" registration mechanism on the Module trait. Deferred until a second module needs it.

### FR-6: Module Registration & Integration

**The module SHALL integrate with the existing module system following established patterns.**

| ID | Requirement | Acceptance Criteria |
|----|------------|-------------------|
| FR-6.1 | Add `cp-mod-entities` to `Cargo.toml` workspace members list. | Compiles as part of workspace. |
| FR-6.2 | Add `EntitiesModule` entry in `src/modules/mod.rs::all_modules()`, positioned after `SearchModule`. | Module appears in `all_modules()` vec. |
| FR-6.3 | `id() → "entities"`, `name() → "Entities"`, `description() → "Persistent relational entity database (SQLite)"`. | Correct metadata. |
| FR-6.4 | `is_global() → true` — entities are project-wide, stored in `config.json`. | Data persists in shared config. |
| FR-6.5 | `is_core() → false` — module can be deactivated. | `module_toggle(deactivate entities)` works. |
| FR-6.6 | `dependencies() → &["search"]` — requires search module for Meilisearch sync. | Dependency validation passes. Cannot activate entities without search. |
| FR-6.7 | Register `Kind::ENTITIES` constant in `cp-base/src/state/context.rs` following the pattern of `Kind::MEMORY`, `Kind::TODO`, etc. | `Kind::ENTITIES` available for panel type. |
| FR-6.8 | `context_type_metadata()` returns `TypeMeta { context_type: "entities", icon_id: "entities", is_fixed: true, needs_cache: false, fixed_order: Some(5), display_name: "entities", short_name: "entities", needs_async_wait: false }`. | Panel ordering and sidebar icon correct. |
| FR-6.9 | `tool_category_descriptions() → vec![("Entity", "Persistent relational entity database")]`. | Category appears in Tools panel. |
| FR-6.10 | `overview_context_section()` returns `"Entities: N tables, M rows\n"` (or `None` if no user tables). | Overview section present when entities exist. |
| FR-6.11 | Create `yamls/tools/entities.yaml` with tool description. Update compile-time YAML validation test in `cp-base/src/lib.rs` (currently validates 19 tool files — becomes 20). | YAML validation test passes with 20 files. |
| FR-6.12 | Tool visualizer for `entity_sql`: color table headers, highlight row counts, dim NULLs. | Conversation view renders entity results with visual structure. |

---

## 6. Non-Functional Requirements

### NFR-1: Performance

| ID | Requirement | Rationale |
|----|------------|-----------|
| NFR-1.1 | `entity_sql` tool execution (excluding Meilisearch sync) SHALL complete in < 200ms for queries on tables with ≤ 10,000 rows. | SQLite is fast for small datasets. Entity tables are expected to be small (hundreds to low thousands of rows). |
| NFR-1.2 | Panel `refresh()` (schema introspection) SHALL complete in < 50ms. | Panel refresh runs on the main thread at ~28fps render rate. Must not stall the UI. |
| NFR-1.3 | Meilisearch sync SHALL be fire-and-forget with no blocking on the tool execution path. | Follows `sync_logs_to_meilisearch` pattern. Meilisearch processes tasks asynchronously. |
| NFR-1.4 | Connection open + PRAGMA setup SHALL complete in < 10ms. | Connection is created per tool call. Must be negligible overhead. SQLite file open is ~1ms. |

### NFR-2: Reliability & Data Integrity

| ID | Requirement | Rationale |
|----|------------|-----------|
| NFR-2.1 | SQLite ACID guarantees SHALL be preserved: all multi-statement batches execute atomically via implicit transactions. | Partial writes must not corrupt entity state. |
| NFR-2.2 | WAL mode SHALL be enabled to prevent corruption from unexpected process termination. | WAL provides crash safety and concurrent read access. |
| NFR-2.3 | `busy_timeout = 5000` SHALL prevent indefinite blocking if another connection holds a write lock. | Multiple workers sharing the DB must not deadlock. |
| NFR-2.4 | On module load, run `PRAGMA integrity_check`. On failure, log a warning and re-create the DB. | Corrupt DB must not crash Context Pilot. Self-healing behavior. |
| NFR-2.5 | Meilisearch unavailability SHALL NOT prevent entity tool execution. SQL operations work independently of Meilisearch. | Search is a discovery enhancement, not a hard dependency for CRUD. |

### NFR-3: Maintainability & Code Quality

| ID | Requirement | Rationale |
|----|------------|-----------|
| NFR-3.1 | No source file SHALL exceed 500 lines. | Enforced project-wide by CB1 (`check-file-lengths.sh`). |
| NFR-3.2 | All workspace clippy lints SHALL pass at the `forbid` level configured in `Cargo.toml`. | The project uses ~961 clippy lints. No exceptions unless registered in `allowed-lint-exceptions.yaml`. |
| NFR-3.3 | `cargo fmt --check` SHALL pass. | Enforced by CB6 (`rust-fmt`). |
| NFR-3.4 | All public functions SHALL have doc comments with `# Errors` sections where applicable. | Project convention. |
| NFR-3.5 | Implementation SHALL follow the `cp-mod-memory` module structure as the primary reference pattern. | Consistency across modules reduces cognitive load. |

### NFR-4: Dependency Budget

| ID | Requirement | Rationale |
|----|------------|-----------|
| NFR-4.1 | The module SHALL add ≤ 8 new transitive dependencies to the workspace. | Post-typst cleanup brought total to 348. Budget allows growth to ~356. |
| NFR-4.2 | The only new direct dependency (beyond workspace crates) SHALL be `rusqlite` with `features = ["bundled", "column_decltype"]`. | `bundled` compiles SQLite from C source (via `cc`, already in tree). `column_decltype` enables type introspection for panel display. |
| NFR-4.3 | `rusqlite` SHALL be declared as a workspace dependency in the root `Cargo.toml`. | Follows workspace dependency management convention. |

### NFR-5: Concurrency & Multi-Worker Safety

| ID | Requirement | Rationale |
|----|------------|-----------|
| NFR-5.1 | Multiple workers SHALL be able to read the entity DB concurrently without blocking. | WAL mode supports unlimited concurrent readers. |
| NFR-5.2 | Write operations SHALL be serialized by SQLite's built-in locking with a 5-second busy timeout. | Low write frequency makes contention unlikely. Timeout prevents deadlocks. |
| NFR-5.3 | No `rusqlite::Connection` SHALL be stored in `EntitiesState`. Connections are created per operation. | `Connection` is `!Send`. Per-call creation avoids threading issues. |

---

## 7. Technical Design

### 7.1 Crate Structure

```
crates/cp-mod-entities/
├── Cargo.toml
└── src/
    ├── lib.rs          # Module trait impl (~200 lines)
    ├── db.rs           # Connection factory, PRAGMAs, bootstrap, schema introspection (~250 lines)
    ├── tools.rs        # entity_sql execution, SQL classification, result formatting (~350 lines)
    ├── panel.rs        # Fixed Entities panel: blocks, context, refresh (~200 lines)
    ├── sync.rs         # Meilisearch sync: table→documents, upsert, delete (~200 lines)
    └── types.rs        # EntitiesState, SchemaInfo, TableInfo, ColumnInfo (~100 lines)
```

### 7.2 Cargo.toml

```toml
[package]
name = "cp-mod-entities"
version = "0.1.0"
edition.workspace = true

[dependencies]
cp-base = { path = "../cp-base" }
cp-render = { path = "../cp-render" }
cp-mod-search = { path = "../cp-mod-search" }
rusqlite = { workspace = true, features = ["bundled", "column_decltype"] }
serde_json = { workspace = true }
crossterm = { workspace = true }
log = { workspace = true }
```

### 7.3 State Management (`types.rs`)

```rust
use std::path::PathBuf;

/// Runtime state stored in the State TypeMap via `state.set_ext()`.
pub struct EntitiesState {
    /// Absolute path to the SQLite database file.
    pub db_path: PathBuf,
    /// Cached schema for panel rendering (refreshed after each tool call).
    pub schema_cache: Option<SchemaCache>,
    /// Meilisearch port (copied from SearchState on init, 0 = unavailable).
    pub meili_port: u16,
    /// Meilisearch API key.
    pub meili_key: String,
    /// Meilisearch entities index UID: "cp_{hash}_entities".
    pub entities_index_uid: String,
}

/// Cached schema information for fast panel rendering.
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

### 7.4 Connection Factory (`db.rs`)

```rust
/// Open a connection to the entity database with all PRAGMAs set.
/// Creates the DB file and _meta table on first use.
pub(crate) fn open_connection(db_path: &Path) -> Result<rusqlite::Connection, String> {
    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("Failed to open entity DB: {e}"))?;
    
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA journal_size_limit = 67108864;"
    ).map_err(|e| format!("PRAGMA setup failed: {e}"))?;
    
    // Bootstrap _meta table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _meta (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version', '1');
         INSERT OR IGNORE INTO _meta (key, value) VALUES ('created_at', datetime('now'));"
    ).map_err(|e| format!("Bootstrap failed: {e}"))?;
    
    Ok(conn)
}
```

### 7.5 SQL Classification

```rust
/// Classify a SQL statement as read-only or write.
fn is_read_only(sql: &str) -> bool {
    let trimmed = sql.trim();
    let upper = trimmed.to_uppercase();
    upper.starts_with("SELECT")
        || upper.starts_with("EXPLAIN")
        || upper.starts_with("PRAGMA")
        || (upper.starts_with("WITH") && !upper.contains("INSERT")
            && !upper.contains("UPDATE") && !upper.contains("DELETE"))
}
```

### 7.6 Multi-Statement Splitting

```rust
/// Split SQL on `;` boundaries, respecting single-quoted string literals.
fn split_statements(sql: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    
    for (i, ch) in sql.char_indices() {
        match ch {
            '\'' => in_string = !in_string,
            ';' if !in_string => {
                let stmt = sql[start..i].trim();
                if !stmt.is_empty() {
                    statements.push(stmt);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    // Last statement (no trailing semicolon)
    let last = sql[start..].trim();
    if !last.is_empty() {
        statements.push(last);
    }
    statements
}
```

### 7.7 Meilisearch Document Schema

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

**Primary key:** `"id"` — composite of `{table_name}__{sqlite_rowid}`.

**Index settings:**
```rust
fn entities_index_settings() -> serde_json::Value {
    serde_json::json!({
        "searchableAttributes": ["_all_text"],
        "filterableAttributes": ["entity_table"],
        "sortableAttributes": [],
        "typoTolerance": {
            "enabled": true,
            "minWordSizeForTypos": { "oneTypo": 4, "twoTypos": 8 }
        }
    })
}
```

### 7.8 Tool Visualizer

```rust
fn visualize_entity_output(content: &str, width: usize) -> Vec<Block> {
    // Color scheme:
    // - Table headers (| col | col |): Semantic::Accent
    // - Row counts "(N rows)": Semantic::Success  
    // - "Error:": Semantic::Warning
    // - "NULL": Semantic::Muted + dimmed
    // - Schema lines: Semantic::Code
}
```

---

## 8. Integration Points

| Integration | Module | Direction | Mechanism |
|-------------|--------|-----------|-----------|
| Meilisearch server | `cp-mod-search` | Read | Read `SearchState.persist.{port, master_key, project_hash}` from TypeMap |
| Meilisearch client | `cp-mod-search` | Use | Import `cp_mod_search::meili::client::MeiliClient` (already `pub(crate)` — needs `pub` exposure or re-export) |
| Search tool scope | `cp-mod-search` | Modify | Add `"entities"` scope in `cp-mod-search/src/tools.rs` |
| Module registry | `src/modules/mod.rs` | Modify | Add `EntitiesModule` to `all_modules()` |
| Kind registry | `cp-base/src/state/context.rs` | Modify | Add `Kind::ENTITIES` constant |
| YAML validation | `cp-base/src/lib.rs` | Modify | Update tool file count: 19 → 20 |
| Workspace | `Cargo.toml` | Modify | Add member + workspace dependency |

**Note on `MeiliClient` visibility:**
Currently `MeiliClient` is `pub(crate)` in `cp-mod-search`. The entities module needs to construct a `MeiliClient` for sync. Options:
1. **Re-export** a `pub fn create_meili_client(state: &State) -> Option<MeiliClient>` from `cp-mod-search`.
2. **Duplicate** the HTTP calls in `cp-mod-entities/src/sync.rs` using `reqwest` directly.
3. **Move** `MeiliClient` to `cp-base` as shared infrastructure.

**Recommendation:** Option 1 — minimal change, clean API boundary. Add a `pub fn meili_client(state: &State) -> Option<MeiliClient>` to `cp-mod-search/src/lib.rs`.

---

## 9. Migration & Rollout

### Phase 1: Crate scaffold (no functional changes)
- Create `crates/cp-mod-entities/` with `Cargo.toml`, empty `lib.rs`
- Add to workspace members
- Verify compilation

### Phase 2: Core (DB + Tool + Panel)
- Implement `db.rs` (connection, bootstrap, schema introspection)
- Implement `types.rs` (state types)
- Implement `tools.rs` (SQL execution, result formatting)
- Implement `panel.rs` (schema overview)
- Implement `lib.rs` (Module trait)
- Register module in `mod.rs`, add Kind constant, add YAML tool def
- Run all 6 callbacks ✓

### Phase 3: Meilisearch integration
- Implement `sync.rs` (table → Meilisearch documents)
- Create entities index on module init
- Wire sync into tool execution path
- Expose `meili_client()` from search module

### Phase 4: Search scope integration
- Add `"entities"` scope to search tool
- Wire entity result parsing in search tool
- Update search YAML description

### Phase 5: Polish
- Tool visualizer
- Overview section
- Documentation

---

## 10. Open Questions & Decisions

| # | Question | Options | Recommendation | Status |
|---|----------|---------|---------------|--------|
| Q1 | Shared DB across workers? | A) Shared file with WAL. B) Per-worker copies. | **A — Shared.** WAL handles concurrent reads. Write contention unlikely (low frequency). | Decided |
| Q2 | Git tracking for `.db` file? | A) Gitignore. B) Auto-export `.sql` dump. C) Both. | **A for v1.** Binary files in git are messy. AI can recreate schema. Add export later. | Decided |
| Q3 | Search scope integration model? | A) `entities` scope in search tool. B) Auto-include in `all`. C) Both. | **C — Both.** `scope="all"` includes entities; `scope="entities"` for focused queries. | Decided |
| Q4 | Schema conventions in tool description? | A) No guidance. B) Suggest patterns. C) Enforce patterns. | **B — Suggest.** Include node/edge pattern suggestion in YAML description. No enforcement. | Decided |
| Q5 | Inline result row limit? | 20 / 50 / 100 | **50 rows.** Balances context usage vs utility. > 50 → dynamic panel. | Decided |
| Q6 | Transaction semantics? | A) Implicit per-call. B) Explicit BEGIN/COMMIT across calls. | **A — Implicit.** Each tool call = one transaction. No cross-call state. Multi-statement within one call is atomic. | Decided |
| Q7 | `MeiliClient` visibility? | A) Re-export function. B) Duplicate HTTP. C) Move to cp-base. | **A — Re-export.** Minimal change. Clean boundary. | Decided |
| Q8 | Embedder configuration for entities index? | A) Reuse Voyage AI. B) Skip embeddings. | **Open.** Entities are short text — keyword search may suffice. Voyage API cost for small docs is minimal. Decide during Phase 3. | Open |

---

## 11. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **SQLite C compilation fails on cross-compilation target** | Low | High | `cc` crate already handles OpenSSL cross-compilation in our CI. SQLite amalgamation is simpler than OpenSSL. Test in CI matrix early (Phase 1). |
| **`rusqlite` adds more than 8 transitive deps** | Low | Medium | Audit `cargo tree -p rusqlite --depth 1` before merging. If over budget, evaluate `features` to trim. |
| **Large entity tables (>10K rows) cause slow Meilisearch sync** | Low | Medium | v1 re-indexes all tables on every write. If this becomes a bottleneck, optimize to sync only affected tables via SQL parsing. |
| **Concurrent write contention from multiple workers** | Low | Low | SQLite serializes writes with 5s timeout. Entity writes are infrequent. If contention appears, add retry logic. |
| **AI creates poorly structured schemas** | Medium | Low | Schema conventions in tool description guide the AI. The panel shows schema, enabling self-correction. Not our problem to enforce. |
| **DB corruption from unexpected termination** | Very Low | Medium | WAL mode provides crash safety. `integrity_check` on load detects and recovers from corruption via re-creation. |

---

## 12. Future Extensions (not v1)

| Extension | Description | Trigger |
|-----------|------------|---------|
| **Entity visualization** | Render graph structure in the panel (ASCII or IR-based) | User demand |
| **Schema migrations** | Track schema version changes, provide `entity_migrate` tool | Projects with long-lived entity schemas |
| **Cross-project entities** | Global entity DB at `~/.context-pilot/entities.db` | Multi-project workflows |
| **Export/import tools** | `entity_export` (SQL dump, CSV, JSON), `entity_import` | Data portability |
| **Spine notifications** | Entity changes fire spine notifications for reactive workflows | Automation use cases |
| **Virtual tables** | SQLite virtual tables that read from project files or Meilisearch | Advanced data integration |
| **FTS5 integration** | Full-text search columns within SQLite (complement Meilisearch) | Large text fields in entities |

---

## 13. Appendix

### A. Reference Implementations

| Pattern | Reference File | What to Copy |
|---------|---------------|-------------|
| Module trait impl | `crates/cp-mod-memory/src/lib.rs` | init/save/load, tool defs, panel creation, visualizers |
| Tool execution | `crates/cp-mod-memory/src/tools.rs` | Input parsing, validation, result building, `flame!()` |
| Fixed panel | `crates/cp-mod-memory/src/panel.rs` | `blocks()`, `context()`, `refresh()`, `needs_cache()` |
| Meilisearch index creation | `crates/cp-mod-search/src/meili/bootstrap.rs` | `ensure_indexes()`, settings, embedder config |
| Meilisearch document upsert | `crates/cp-mod-search/src/lib.rs::sync_logs_to_meilisearch` | Fire-and-forget `add_documents()` pattern |
| Meilisearch client | `crates/cp-mod-search/src/meili/client.rs` | `MeiliClient::new()`, `add_documents()`, `delete_documents_by_filter()` |
| Search tool scope | `crates/cp-mod-search/src/tools.rs` | `scope` parameter handling, multi-index search |
| Module registration | `src/modules/mod.rs` | `all_modules()`, `use` imports, `Box::new(Module)` |
| Kind constant | `crates/cp-base/src/state/context.rs` | `pub const ENTITIES: &str = "entities";` pattern |
| Tool YAML | `yamls/tools/memory.yaml` | Description format, parameter definitions |
| YAML validation test | `crates/cp-base/src/lib.rs` | Compile-time `include_str!` validation count |

### B. Dependency Audit (Pre-Implementation)

Run before merging Phase 1:
```bash
cargo tree -p rusqlite --features bundled --depth 2 --no-default-features
```
Expected output: rusqlite → libsqlite3-sys → cc (already in tree), plus hashlink, fallible-iterator, fallible-streaming-iterator. Total ≤ 8 new crates.
