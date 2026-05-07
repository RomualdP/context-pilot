# Search Module (`cp-mod-search`)

Full-text search across project files and conversation logs, powered by an embedded [Meilisearch](https://www.meilisearch.com/) server.

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│  search tool │────▶│ MeiliClient  │────▶│   Meilisearch   │
│  (tools.rs)  │     │  (client.rs) │     │   (embedded)    │
└─────────────┘     └──────────────┘     └────────┬────────┘
                                                   │
┌─────────────┐     ┌──────────────┐               │
│ File Watcher │────▶│   Indexer    │───────────────┘
│   (notify)   │     │ (indexer.rs) │
└─────────────┘     └──────────────┘
        │                    │
        │              ┌─────┴──────┐
        │              │  Splitter  │
        │              │ Chain      │
        │              ├────────────┤
        │              │ TreeSitter │
        │              │ FixedSize  │
        └──────────────┴────────────┘
```

### Components

| File | Role |
|------|------|
| `lib.rs` | Module trait implementation, server bootstrap, index creation |
| `server.rs` | Meilisearch binary download, lifecycle (start/stop/reconnect) |
| `client.rs` | HTTP API client (index CRUD, documents, search, stats) |
| `indexer.rs` | Background thread: file watcher → debounce → split → index |
| `splitter/mod.rs` | Splitter trait + chain dispatch |
| `splitter/tree_sitter.rs` | AST-aware chunking (functions, structs, classes) |
| `splitter/fixed_size.rs` | Fallback: 4000-char chunks on line boundaries |
| `tools.rs` | `search` tool execution and result formatting |
| `panel.rs` | Dynamic search result panel rendering |
| `config.rs` | Extension allowlists, path exclusions, index settings |
| `types.rs` | Core types: `SearchState`, `Chunk`, `SearchResult`, etc. |

## Server Management

The Meilisearch server is **global** — shared across all Context Pilot instances on the machine.

- Binary: `~/.context-pilot/meilisearch/bin/meilisearch`
- Data: `~/.context-pilot/meilisearch/data.ms/`
- Config: `~/.context-pilot/meilisearch/` (port, master key, PID)
- Auto-downloaded from GitHub Releases on first use

### Lifecycle

1. **First boot**: Download binary → generate master key → find free port → start server
2. **Reconnect**: Read port file → health check → reuse if healthy, restart otherwise
3. **Per-project**: Create `cp_{hash}_files` and `cp_{hash}_logs` indexes

## Indexes

Two Meilisearch indexes per project, named using an 8-character SHA-256 hash of the project root path:

### Files Index (`cp_{hash}_files`)

| Field | Type | Searchable | Filterable | Sortable |
|-------|------|-----------|------------|----------|
| `id` | string (PK) | — | — | — |
| `content` | string | ✓ | — | — |
| `file_path` | string | ✓ | ✓ | — |
| `extension` | string | — | ✓ | — |
| `chunk_type` | string | — | ✓ | — |
| `chunk_name` | string | ✓ | — | — |
| `line_start` | integer | — | — | — |
| `line_end` | integer | — | — | — |
| `last_modified_ms` | integer | — | ✓ | ✓ |

### Logs Index (`cp_{hash}_logs`)

| Field | Type | Searchable | Filterable | Sortable |
|-------|------|-----------|------------|----------|
| `id` | string (PK) | — | — | — |
| `content` | string | ✓ | — | — |
| `datetime` | string | ✓ | — | — |
| `timestamp_ms` | integer | — | ✓ | ✓ |
| `importance` | string | — | ✓ | — |
| `tags` | array | ✓ | ✓ | — |

## File Processing Pipeline

```
File event → Filter → Read → Split → Build docs → Batch upsert
```

### Filtering

Files are filtered in order:
1. **Path exclusions**: `.git/`, `node_modules/`, `target/`, `.context-pilot/` (except logs)
2. **Extension allowlist**: ~50 extensions covering code, config, docs, data
3. **Size cap**: 1 MB maximum
4. **Symlink check**: Skip symbolic links
5. **Binary detection**: Check for null bytes in first 8 KB

### Splitting

The splitter chain tries in order:
1. **Tree-sitter**: AST parsing for supported languages (Rust, Python, JS, TS, Go, Java, C, C++)
   - Extracts functions, structs, classes, enums, impl blocks
   - Preserves semantic boundaries
2. **Fixed-size fallback**: 4000-character chunks split on line boundaries
   - Used for unsupported languages and non-code files

### Indexing

- **New file**: Split → build index documents → batch upsert
- **Modified file**: Delete all chunks by path → re-split → re-insert
- **Deleted file**: Delete all chunks by path
- **Debounce**: 200ms quiet period before processing batched events

## Search Tool

One unified tool: `search`

### Parameters

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `query` | string | (required) | Search query |
| `scope` | enum | `"all"` | `"all"`, `"project"`, `"logs"` |
| `path_prefix` | string | — | Filter by file path prefix |
| `extension` | string | — | Filter by file extension |
| `sort` | enum | `"relevance"` | `"relevance"`, `"date_asc"`, `"date_desc"` |
| `from_date` | string | — | ISO 8601 date lower bound |
| `to_date` | string | — | ISO 8601 date upper bound |
| `include_context` | bool | `true` | `false` = panel-only ("peek" mode) |
| `limit` | integer | `20` | Max results per scope (1-50) |

### Result Panel

Results appear as a dynamic `search_result` panel with:
- **File results**: path, line range, chunk type/name, highlighted content
- **Log results**: datetime, importance, tags, content

## Ctrl+I Overlay

Press `Ctrl+I` to toggle the search indexing status overlay:
- Server status and port
- Chunks indexed / files indexed
- Queue depth and error count
- Last activity timestamp

## Log Integration

The search module indexes logs via filesystem watching — **zero coupling** with `cp-mod-logs`:

1. `cp-mod-logs` writes JSON chunk files to `.context-pilot/logs/`
2. The file watcher detects changes
3. The indexer reads the JSON, builds log documents, and upserts to Meilisearch
4. The `search` tool queries the logs index with `scope=logs`

### Log Schema (v2)

Logs use the new v2 schema with tags and importance (no more parent/children hierarchy):

```json
{
  "id": "L42",
  "content": "Decided to use tree-sitter for chunking",
  "timestamp_ms": 1746537600000,
  "datetime": "2026-05-06T12:00:00+00:00",
  "importance": "high",
  "tags": ["decision", "architecture"]
}
```

## Configuration

Extension allowlists and path exclusions are defined in `config.rs`. Future: `search.toml` config file for per-project overrides.

### Supported Tree-sitter Languages

| Language | Grammar Crate |
|----------|--------------|
| Rust | `tree-sitter-rust` |
| Python | `tree-sitter-python` |
| JavaScript | `tree-sitter-javascript` |
| TypeScript | `tree-sitter-typescript` |
| Go | `tree-sitter-go` |
| Java | `tree-sitter-java` |
| C | `tree-sitter-c` |
| C++ | `tree-sitter-cpp` |

## Design Decisions

Full design history in [`docs/design-meilisearch.md`](design-meilisearch.md) (9 rounds of decisions).

Key choices:
- **Embedded server** (not cloud/Docker) — works offline, zero setup
- **Global server** — shared across projects, per-project indexes
- **Tree-sitter AST chunking** — semantic boundaries, not arbitrary cuts
- **File watcher** — real-time incremental updates, not periodic scans
- **Zero coupling with logs** — search discovers logs via filesystem
- **Unified search tool** — one tool for both files and logs
