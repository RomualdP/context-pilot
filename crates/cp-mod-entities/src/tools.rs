//! SQL execution engine: classification, splitting, execution, error enrichment.

use cp_base::state::context::Kind;
use cp_base::state::runtime::State;
use cp_base::tools::{ToolResult, ToolUse};

use crate::types::SchemaCache;
use crate::{db, migrations};

// =============================================================================
// SQL classification
// =============================================================================

/// Broad category of a SQL statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SqlKind {
    /// `SELECT`, `EXPLAIN`, `PRAGMA` (read-only).
    Select,
    /// `INSERT`, `UPDATE`, `DELETE` (data manipulation).
    Dml,
    /// `CREATE`, `ALTER`, `DROP` (schema change).
    Ddl,
}

/// Classify a SQL statement by its first keyword.
///
/// CTEs (`WITH ... SELECT` vs `WITH ... INSERT`) are detected by scanning
/// for DML/DDL keywords after the CTE. Default is [`SqlKind::Dml`] (conservative).
pub(crate) fn classify(sql: &str) -> SqlKind {
    let upper = sql.trim().to_uppercase();
    let first_word: String = upper.chars().take_while(char::is_ascii_alphabetic).collect();

    match first_word.as_str() {
        "SELECT" | "EXPLAIN" | "PRAGMA" => SqlKind::Select,
        "CREATE" | "ALTER" | "DROP" => SqlKind::Ddl,
        "WITH" => classify_cte(&upper),
        _ => SqlKind::Dml, // conservative: INSERT/UPDATE/DELETE/REPLACE and unknown
    }
}

/// Classify a CTE by scanning for DML/DDL keywords after `WITH`.
fn classify_cte(upper: &str) -> SqlKind {
    // Look for DDL keywords
    if upper.contains("CREATE ") || upper.contains("ALTER ") || upper.contains("DROP ") {
        return SqlKind::Ddl;
    }
    // Look for DML keywords
    if upper.contains("INSERT ") || upper.contains("UPDATE ") || upper.contains("DELETE ") || upper.contains("REPLACE ")
    {
        return SqlKind::Dml;
    }
    SqlKind::Select
}

// =============================================================================
// Statement splitting
// =============================================================================

/// Split SQL on `;` while respecting single-quoted string literals.
///
/// Handles `''` escape sequences inside strings.
pub(crate) fn split_statements(sql: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars.get(i).copied().unwrap_or_default();

        if in_string {
            if ch == '\'' {
                // Check for escaped quote ('')
                if chars.get(i.saturating_add(1)).copied() == Some('\'') {
                    i = i.saturating_add(2);
                    continue;
                }
                in_string = false;
            }
        } else if ch == '\'' {
            in_string = true;
        } else if ch == ';' {
            let stmt = sql.get(start..i).unwrap_or_default().trim();
            if !stmt.is_empty() {
                results.push(stmt);
            }
            start = i.saturating_add(1);
        }

        i = i.saturating_add(1);
    }

    // Last statement (no trailing semicolon)
    let tail = sql.get(start..).unwrap_or_default().trim();
    if !tail.is_empty() {
        results.push(tail);
    }

    results
}

// =============================================================================
// Main execution entry point
// =============================================================================

/// Execute the `entity_sql` tool call.
///
/// Opens a per-call connection, classifies the SQL, executes, formats the
/// result, and handles post-execution bookkeeping (panel refresh, migration
/// capture, dump regeneration, schema cache update).
pub(crate) fn execute(tool: &ToolUse, state: &mut State) -> ToolResult {
    let _fg = cp_base::flame!("entity_sql");

    let sql = tool.input.get("sql").and_then(serde_json::Value::as_str).unwrap_or_default();

    if sql.trim().is_empty() {
        return err(tool, "SQL parameter is empty.");
    }

    let es = crate::types::EntitiesState::get(state);
    let db_path = es.db_path.clone();
    let dump_path = es.dump_path.clone();
    let migrations_dir = es.migrations_dir.clone();

    let conn = match db::open(&db_path) {
        Ok(c) => c,
        Err(e) => return err(tool, &e),
    };

    let kind = classify(sql);

    let result_content = match kind {
        SqlKind::Select => execute_select(&conn, sql, state),
        SqlKind::Dml => execute_dml(&conn, sql),
        SqlKind::Ddl => execute_ddl(&conn, sql, &dump_path, &migrations_dir),
    };

    // Handle errors
    let (content, is_error) = match result_content {
        Ok(text) => (text, false),
        Err(e) => {
            let schema = db::introspect(&conn, &db_path);
            (enrich_error(&e, &schema), true)
        }
    };

    // Post-execution: refresh schema cache + touch panel
    let fresh_cache = db::introspect(&conn, &db_path);
    let es_mut = crate::types::EntitiesState::get_mut(state);
    es_mut.schema_cache = Some(fresh_cache);
    state.touch_panel(Kind::ENTITIES);

    ToolResult {
        tool_use_id: tool.id.clone(),
        content,
        display: None,
        tldr: None,
        is_error,
        preserves_tempo: false,
        tool_name: tool.name.clone(),
    }
}

// =============================================================================
// Per-kind execution
// =============================================================================

/// Execute a SELECT / EXPLAIN / PRAGMA query and format results as markdown.
fn execute_select(conn: &Connection, sql: &str, state: &State) -> Result<String, String> {
    let stmts = split_statements(sql);
    let last = stmts.last().copied().unwrap_or(sql);

    // Execute all but the last (side-effect statements like pragmas)
    for stmt in stmts.iter().take(stmts.len().saturating_sub(1)) {
        conn.execute_batch(stmt).map_err(|e| format!("{e}"))?;
    }

    // Execute the last statement as a query
    query_to_markdown(conn, last, state)
}

/// Execute a DML statement. Handles `RETURNING` clauses.
fn execute_dml(conn: &Connection, sql: &str) -> Result<String, String> {
    let stmts = split_statements(sql);
    let upper = sql.to_uppercase();
    let has_returning = upper.contains("RETURNING");

    // Execute all statements
    let mut total_affected = 0usize;
    for (i, stmt) in stmts.iter().enumerate() {
        let is_last = i == stmts.len().saturating_sub(1);

        if is_last && has_returning {
            // Last statement with RETURNING — format as table
            let mut prep = conn.prepare(stmt).map_err(|e| format!("{e}"))?;
            let col_names: Vec<String> = prep.column_names().iter().map(|s| (*s).to_string()).collect();
            let mut rows_data: Vec<Vec<String>> = Vec::new();

            let mut rows = prep.query([]).map_err(|e| format!("{e}"))?;
            while let Some(row) = rows.next().map_err(|e| format!("{e}"))? {
                let mut vals = Vec::with_capacity(col_names.len());
                for idx in 0..col_names.len() {
                    vals.push(format_cell(row, idx));
                }
                rows_data.push(vals);
            }

            let count = rows_data.len();
            let table = format_markdown_table(&col_names, &rows_data);
            if total_affected > 0 {
                return Ok(format!("{total_affected} row(s) affected.\n\n{table}\n\n({count} returned)"));
            }
            return Ok(format!("{table}\n\n({count} returned)"));
        }

        let affected = conn.execute(stmt, []).map_err(|e| format!("{e}"))?;
        total_affected = total_affected.saturating_add(affected);
    }

    Ok(format!("{total_affected} row(s) affected."))
}

/// Execute DDL. Writes migration + regenerates dump.
fn execute_ddl(conn: &Connection, sql: &str, dump_path: &Path, migrations_dir: &Path) -> Result<String, String> {
    conn.execute_batch(sql).map_err(|e| format!("{e}"))?;

    // Write migration file
    let filename = migrations::write_migration(conn, migrations_dir, sql)?;

    // Regenerate full dump
    if let Err(e) = db::dump_to_file(conn, dump_path) {
        log::warn!("Failed to regenerate dump after DDL: {e}");
    }

    Ok(format!("Schema updated. Migration saved: {filename}"))
}

// =============================================================================
// Query formatting
// =============================================================================

/// Execute a query and format results as a markdown table.
fn query_to_markdown(conn: &Connection, sql: &str, state: &State) -> Result<String, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("{e}"))?;
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| (*s).to_string()).collect();
    let mut rows_data: Vec<Vec<String>> = Vec::new();

    let mut rows = stmt.query([]).map_err(|e| format!("{e}"))?;
    while let Some(row) = rows.next().map_err(|e| format!("{e}"))? {
        let mut vals = Vec::with_capacity(col_names.len());
        for idx in 0..col_names.len() {
            vals.push(format_cell(row, idx));
        }
        rows_data.push(vals);
    }

    let count = rows_data.len();

    if count == 0 {
        // Provide context about total rows for filtered queries
        let table_hint = extract_table_name(sql);
        if let Some(tbl) = &table_hint {
            let es = crate::types::EntitiesState::get(state);
            if let Some(cache) = &es.schema_cache
                && let Some(info) = cache.tables.iter().find(|t| t.name.eq_ignore_ascii_case(tbl))
            {
                return Ok(format!("0 rows returned. (Table '{}' has {} total rows.)", info.name, info.row_count));
            }
        }
        return Ok("0 rows returned.".to_string());
    }

    // Cap inline results at 50 rows
    if count > 50 {
        let truncated = rows_data.get(..50).unwrap_or(&rows_data);
        let table = format_markdown_table(&col_names, truncated);
        return Ok(format!("{table}\n\n({count} rows, showing first 50)"));
    }

    let table = format_markdown_table(&col_names, &rows_data);
    Ok(format!("{table}\n\n({count} rows)"))
}

/// Format a single cell value for markdown table display.
fn format_cell(row: &rusqlite::Row<'_>, idx: usize) -> String {
    use rusqlite::types::ValueRef;

    let Ok(val) = row.get_ref(idx) else {
        return "NULL".to_string();
    };

    match val {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Blob(b) => format!("[BLOB {} bytes]", b.len()),
    }
}

/// Build a markdown table from column names and row data.
fn format_markdown_table(cols: &[String], rows: &[Vec<String>]) -> String {
    if cols.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    // Header
    out.push_str("| ");
    out.push_str(&cols.join(" | "));
    out.push_str(" |\n");

    // Separator
    out.push('|');
    for _ in cols {
        out.push_str("------|");
    }
    out.push('\n');

    // Rows
    for row in rows {
        out.push_str("| ");
        out.push_str(&row.join(" | "));
        out.push_str(" |\n");
    }

    out
}

/// Try to extract the main table name from a SELECT query.
fn extract_table_name(sql: &str) -> Option<String> {
    let upper = sql.to_uppercase();
    let from_pos = upper.find("FROM ")?;
    let after_from = sql.get(from_pos.saturating_add(5)..)?;
    let name: String = after_from.trim().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if name.is_empty() { None } else { Some(name) }
}

// =============================================================================
// Error enrichment
// =============================================================================

/// Enrich a `SQLite` error message with schema context and fuzzy suggestions.
fn enrich_error(err: &str, schema: &SchemaCache) -> String {
    let mut parts = vec![format!("SQL error: {err}")];

    // Detect "no such table" and suggest closest match
    if let Some(unknown) = extract_after(err, "no such table: ") {
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        if let Some(suggestion) = closest_match(&unknown, &names, 2) {
            parts.push(format!("Did you mean table '{suggestion}'?"));
        }
    }

    // Detect "no such column" and suggest closest match
    if let Some(unknown) = extract_after(err, "no such column: ") {
        let all_cols: Vec<String> =
            schema.tables.iter().flat_map(|t| t.columns.iter().map(|c| c.name.clone())).collect();
        let col_refs: Vec<&str> = all_cols.iter().map(String::as_str).collect();
        if let Some(suggestion) = closest_match(&unknown, &col_refs, 2) {
            parts.push(format!("Did you mean column '{suggestion}'?"));
        }
    }

    // Append schema summary
    if !schema.tables.is_empty() {
        parts.push(String::from("\nCurrent schema:"));
        for table in &schema.tables {
            let cols: Vec<String> = table
                .columns
                .iter()
                .map(|c| {
                    let pk = if c.is_pk { " PK" } else { "" };
                    format!("{} {}{pk}", c.name, c.col_type)
                })
                .collect();
            parts.push(format!("  {} ({}): {}", table.name, table.row_count, cols.join(", ")));
        }
    }

    parts.join("\n")
}

/// Extract text after a pattern in an error message.
fn extract_after(err: &str, pattern: &str) -> Option<String> {
    let pos = err.find(pattern)?;
    let start = pos.saturating_add(pattern.len());
    let rest = err.get(start..)?;
    let word: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if word.is_empty() { None } else { Some(word) }
}

/// Find the closest match within a Levenshtein distance threshold.
fn closest_match<'candidate>(target: &str, candidates: &[&'candidate str], max_dist: usize) -> Option<&'candidate str> {
    let target_lower = target.to_lowercase();
    let mut best: Option<(&str, usize)> = None;

    for candidate in candidates {
        let dist = levenshtein(&target_lower, &candidate.to_lowercase());
        if dist <= max_dist && (best.is_none() || dist < best.map_or(usize::MAX, |(_, d)| d)) {
            best = Some((candidate, dist));
        }
    }

    best.map(|(name, _)| name)
}

/// Levenshtein distance between two strings.
fn levenshtein(source: &str, target: &str) -> usize {
    let source_chars: Vec<char> = source.chars().collect();
    let target_chars: Vec<char> = target.chars().collect();
    let source_len = source_chars.len();
    let target_len = target_chars.len();

    if source_len == 0 {
        return target_len;
    }
    if target_len == 0 {
        return source_len;
    }

    // Use a single row (previous + current)
    let row_len = target_len.saturating_add(1);
    let mut prev: Vec<usize> = (0..row_len).collect();
    let mut curr = vec![0usize; row_len];

    for i in 1..=source_len {
        if let Some(cell) = curr.get_mut(0) {
            *cell = i;
        }
        for j in 1..=target_len {
            let cost = usize::from(source_chars.get(i.saturating_sub(1)) != target_chars.get(j.saturating_sub(1)));

            let del = prev.get(j).copied().unwrap_or(usize::MAX).saturating_add(1);
            let ins = curr.get(j.saturating_sub(1)).copied().unwrap_or(usize::MAX).saturating_add(1);
            let sub = prev.get(j.saturating_sub(1)).copied().unwrap_or(usize::MAX).saturating_add(cost);

            if let Some(cell) = curr.get_mut(j) {
                *cell = del.min(ins).min(sub);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev.get(target_len).copied().unwrap_or_default()
}

// =============================================================================
// Helper
// =============================================================================

/// Build an error `ToolResult`.
fn err(tool: &ToolUse, msg: &str) -> ToolResult {
    ToolResult {
        tool_use_id: tool.id.clone(),
        content: msg.to_string(),
        display: None,
        tldr: None,
        is_error: true,
        preserves_tempo: false,
        tool_name: tool.name.clone(),
    }
}

use rusqlite::Connection;
use std::path::Path;
