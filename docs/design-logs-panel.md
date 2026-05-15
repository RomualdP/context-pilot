# Context Radar — Thought-to-Log Recall Panel

> Status: **DRAFT v0.3** — thought-to-log pivot, decisions locked

## Core Idea

A **fixed panel** that automatically queries Meilisearch's **logs index**
using the AI's recent **task context signals** (from the Think tool) as
semantic search queries.  Results are ranked with **dual adaptive
exponential decay** and presented as YAML to the AI.  Transparent —
refreshes itself whenever the AI thinks.

**Thought → Log retrieval.**  Thoughts capture *current intent* ("I need
to fix the port reconnection bug"); logs capture *past outcomes* ("Fixed
port reconnection bug").  Intent is a better query for recall — it
surfaces relevant past context while the task is still active, not after
it's done.

**Not a code-surfacing panel.**  The AI already controls file search via
the `search` tool.  Context Radar is for automatic recall of **related
past decisions, context, and events** captured in log entries.

## Think Tool Modification

The `Think` tool gains an optional `task_context` parameter:

```json
{
  "thought_body": "Let me investigate why the port doesn't reconnect...",
  "task_context": "Investigating port reconnection failure on TUI reload"
}
```

- **`task_context`** — short (1-2 sentences) description of the current task.
  Stored persistently.  Used as a semantic search query against the logs index.
- **`thought_body`** — unchanged.  Ephemeral scratch space, drops on compaction.

The AI is instructed (via tool description) that `task_context` feeds the
Context Radar panel.  It should describe *what it's working on*, not *how*.

### Storage

Task signals are stored as a **ring buffer of the last 20** in
`SearchPersistData` (the search module's persisted state).  Each signal:

```rust
struct TaskSignal {
    timestamp_ms: u64,
    content: String,     // the task_context value
}
```

Persisted across TUI reloads via `save_module_data` / `load_module_data`.

### Pipeline Integration

In `pipeline.rs`, after a Think tool executes:

1. Extract `task_context` from the tool input (skip if absent/empty)
2. Push a `TaskSignal` into the ring buffer in `SearchState`
3. Trigger a Context Radar panel refresh

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Context Radar Panel                   │
│                   (fixed, ~3000 tokens)                  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Trigger: Think (with task_context)                     │
│           + log_create / Close_conversation_history     │
│           + pre-populate on boot                        │
│                                                         │
│  ┌───────────────────────────────────────┐              │
│  │  Take last N=20 task signals          │              │
│  │  (from Think tool's task_context)     │              │
│  └──────────────┬────────────────────────┘              │
│                 │                                        │
│                 ▼                                        │
│  ┌───────────────────────────────────────┐              │
│  │  For each signal:                     │              │
│  │    Search cp_{hash}_logs index        │              │
│  │    semantic_ratio = 0.7               │              │
│  │    limit = 10 per query               │              │
│  └──────────────┬────────────────────────┘              │
│                 │                                        │
│                 ▼                                        │
│  ┌───────────────────────────────────────┐              │
│  │  Score each result:                   │              │
│  │    score = relevance                  │              │
│  │          × query_decay(signal.ts)     │              │
│  │          × result_decay(log.ts)       │              │
│  └──────────────┬────────────────────────┘              │
│                 │                                        │
│                 ▼                                        │
│  ┌───────────────────────────────────────┐              │
│  │  Dedup by log ID → keep MAX score     │              │
│  │  Sort descending                      │              │
│  │  Take top K=30                        │              │
│  └──────────────┬────────────────────────┘              │
│                 │                                        │
│                 ▼                                        │
│  ┌───────────────────────────────────────┐              │
│  │  Render as YAML panel content         │              │
│  │  (log content + datetime + tags)      │              │
│  └───────────────────────────────────────┘              │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## Decisions

| Parameter           | Value                                           |
|---------------------|-------------------------------------------------|
| Panel name          | **Context Radar**                                |
| Panel type          | Fixed (always present when module active)        |
| Query source        | **Think tool's `task_context`** parameter        |
| Search target       | **Logs only** (AI controls file search)          |
| Search mode         | Hybrid, `semantic_ratio = 0.7`                  |
| Query count (N)     | 20 most recent task signals                      |
| Results per query   | 10 (Meilisearch `limit`)                        |
| Final results (K)   | 30 top-scored after dedup                        |
| Token budget        | ~3000 tokens                                     |
| Content format      | YAML — log content, datetime, tags, importance   |
| Deduplication       | Keep **max** score per unique log ID             |
| Importance scaling  | Recency via adaptive decay (no explicit field)   |
| Decay model         | Adaptive span-based (see below)                  |
| Signal persistence  | Persisted across TUI reloads                     |
| Trigger             | Think (with task_context) + log creation + boot  |
| Boot behavior       | **Pre-populate** from persisted signals + logs   |

## Scoring Formula

### Per-result score

For each task signal `s` (from the N=20 most recent) and each search
result `r` (a log entry) returned by Meilisearch:

```
score(s, r) = meilisearch_relevance(s, r)
            × query_decay(s.timestamp)
            × result_decay(r.timestamp)
```

### Adaptive decay

The half-life **self-adjusts** based on project pace:

```
span = newest_signal.timestamp - oldest_signal.timestamp
half_life = max(span / 2, 5 minutes)
```

If the last 20 signals span 1 hour (active sprint), half-life = 30 min.
If they span 1 week (slow-burn project), half-life = 3.5 days.

**Floor**: 5 minutes prevents division by near-zero during rapid thinking.

### Decay functions

```
query_decay(s)  = exp(-ln(2) × (now - s.timestamp) / half_life)
result_decay(r) = exp(-ln(2) × (now - r.timestamp) / half_life)
```

### Deduplication

When the same log ID appears across results from multiple signals,
**keep the maximum score** (not sum).  Rationale: one strong match is
the signal; summing over-weights generic logs that match everything.

### Final ranking

After dedup, sort by score descending, take top K=30.

## Panel Content Format

Each result is a YAML entry:

```yaml
- id: L42
  datetime: "2026-05-15T14:30:00Z"
  importance: high
  tags: [bug, port-reconnection]
  content: "Port reconnection bug found: load_module_data() used stale persisted port..."
  score: 0.847
```

The panel header shows metadata:

```yaml
# Context Radar — 30 results from 20 signals
# Half-life: 42m (span: 1h24m)
# Last refresh: 2026-05-15T18:22:00Z
# Active signals: "Investigating port reconnection...", "Designing context radar..."
```

## Module Placement

Lives in **`cp-mod-search`** (not `cp-mod-logs`).  Rationale:
- Needs `MeiliClient` access for querying the logs index
- `cp-mod-search` already depends on `cp-mod-logs` (for log sync)
- Adding reverse dependency would create a cycle
- Fundamentally a search-based panel, even though it shows log content

Panel type: `context_radar` — a new fixed panel in the search module.
Signal storage: `Vec<TaskSignal>` in `SearchPersistData` (ring buffer, cap 20).

## Data Flow

```
AI calls Think(task_context="...")
        │
        ▼
  pipeline.rs stores TaskSignal in SearchState (persisted)
        │
        ▼
  Context Radar takes last N=20 signals
        │
        ▼
  For each signal → query cp_{hash}_logs (semantic, ratio=0.7)
        │
        ▼
  Score × query_decay × result_decay → dedup → top K=30
        │
        ▼
  YAML panel content → AI context

Meanwhile, separately:
  Logs created → .context-pilot/logs/ (source of truth)
             → synced to Meilisearch (search index, what radar queries)
```

## Edge Cases

| Case                             | Behavior                                 |
|----------------------------------|------------------------------------------|
| Fewer than 20 signals            | Use all available signals as queries     |
| 0 signals                        | Panel empty ("No task signals yet")      |
| 0 logs in Meilisearch            | Panel empty (nothing to recall)          |
| Think without task_context       | Signal skipped — thought is ephemeral    |
| Meilisearch down                 | Panel empty, graceful degradation        |
| All queries return 0 results     | Panel empty                              |
| Very rapid Think calls (burst)   | Floor half-life at 5 minutes             |
| Signals span < 1 second (same ms)| Use floor half-life (5 min)              |
| Boot with 0 signals but 200 logs | Panel empty until first Think + task_ctx  |
| Boot with persisted signals      | Pre-populate immediately from signals    |

## Performance

- 20 Meilisearch queries × ~10 results each = ~200 hits to score
- Semantic search with Voyage: each query ~100-200ms (API embedding + search)
- Total wall time: **2-4 seconds** per refresh
- Fires after Think (NOT on streaming hot path)
- Can run on background thread, swap panel content atomically

## Implementation Checklist

### Phase 1: Think tool modification
- [ ] Add `task_context` parameter to Think tool definition (core.yaml)
- [ ] Add `TaskSignal` struct to `cp-mod-search/src/types.rs`
- [ ] Add `task_signals: Vec<TaskSignal>` to `SearchPersistData`
- [ ] In `pipeline.rs`: after Think executes, extract `task_context`, push signal
- [ ] Update Think tool description to explain `task_context` feeds Context Radar

### Phase 2: Context Radar panel
- [ ] New file: `cp-mod-search/src/radar.rs` — query + score + rank logic
- [ ] Register `context_radar` panel type in search module
- [ ] Implement `Panel` trait: `context_content()` returns YAML
- [ ] Add to `fixed_panel_types()` and `fixed_panel_defaults()`
- [ ] Wire refresh trigger: after Think (with task_context) + log creation

### Phase 3: Panel rendering
- [ ] Implement adaptive decay calculation (span-based half-life)
- [ ] Score formula: relevance × query_decay × result_decay
- [ ] Dedup by log ID (keep max)
- [ ] YAML output with header metadata
- [ ] Boot pre-population from persisted signals

### Phase 4: Testing
- [ ] Create logs, then Think with task_context → verify recall
- [ ] Test adaptive decay with varying signal time spans
- [ ] Test boot pre-population
- [ ] Test edge cases (0 signals, 0 logs, Meilisearch down)
- [ ] Verify token budget (~3000 tokens)
