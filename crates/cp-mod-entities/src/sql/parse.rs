//! SQL statement parsing: classification + UTF-8-safe statement splitting.
//!
//! These helpers run *before* the SQL reaches `rusqlite`. The splitter must be
//! byte-offset correct: slicing the original `&str` with character indices
//! silently truncates statements that contain multi-byte UTF-8 characters
//! (issue #113), so all slicing here is driven by [`str::char_indices`].

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
/// Leading SQL comments (`--` line and `/* */` block) are stripped before
/// classification. CTEs (`WITH ... SELECT` vs `WITH ... INSERT`) are detected
/// by scanning for DML/DDL keywords after the CTE. Default is [`SqlKind::Dml`]
/// (conservative).
pub(crate) fn classify(sql: &str) -> SqlKind {
    let stripped = strip_leading_comments(sql);
    let upper = stripped.trim().to_uppercase();
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

/// Strip leading SQL comments from a string.
///
/// Removes `--` line comments and `/* ... */` block comments that appear
/// before the first actual SQL keyword. Handles multiple consecutive comments.
pub(crate) fn strip_leading_comments(sql: &str) -> &str {
    let mut s = sql.trim_start();
    loop {
        if s.starts_with("--") {
            // Skip to end of line
            s = s.find('\n').map_or("", |pos| s.get(pos.saturating_add(1)..).unwrap_or(""));
            s = s.trim_start();
        } else if s.starts_with("/*") {
            // Skip to closing */
            s = s.get(2..).unwrap_or("").find("*/").map_or("", |pos| s.get(pos.saturating_add(4)..).unwrap_or(""));
            s = s.trim_start();
        } else {
            break;
        }
    }
    s
}

// =============================================================================
// Statement splitting
// =============================================================================

/// Split SQL on `;` while respecting single-quoted string literals.
///
/// Handles `''` escape sequences inside strings.
///
/// # UTF-8 correctness
///
/// Slicing is driven by the **byte offsets** yielded by [`str::char_indices`],
/// never by character counts. A previous implementation collected
/// `Vec<char>` and used the character index to slice the original `&str`,
/// which truncated statements containing multi-byte characters in any
/// non-final position (issue #113). Each accented Latin character contributes
/// one extra byte, so the slice fell short by exactly that many bytes.
pub(crate) fn split_statements(sql: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut start = 0; // BYTE offset of the current statement's first char
    let mut in_string = false;
    let mut chars = sql.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if in_string {
            if ch == '\'' {
                // Check for escaped quote ('') — consume the second quote and stay in-string
                if chars.peek().map(|&(_, next)| next) == Some('\'') {
                    let _consumed = chars.next();
                    continue;
                }
                in_string = false;
            }
        } else if ch == '\'' {
            in_string = true;
        } else if ch == ';' {
            let stmt = sql.get(start..idx).unwrap_or_default().trim();
            if !stmt.is_empty() && !strip_leading_comments(stmt).is_empty() {
                results.push(stmt);
            }
            // `;` is ASCII (1 byte) but use len_utf8() for principled byte arithmetic.
            start = idx.saturating_add(ch.len_utf8());
        }
    }

    // Last statement (no trailing semicolon)
    let tail = sql.get(start..).unwrap_or_default().trim();
    if !tail.is_empty() && !strip_leading_comments(tail).is_empty() {
        results.push(tail);
    }

    results
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for issue #113: multi-byte UTF-8 in a non-final `VALUES` row
    /// must not truncate the statement. A char-indexed slice into a byte-indexed
    /// `&str` cut the tail short by the number of extra UTF-8 bytes.
    #[test]
    fn multibyte_non_final_rows_not_truncated() {
        let sql = "INSERT INTO t(a,b) VALUES\n\
                   ('s1','Réserve légale'),\n\
                   ('s2','Disponibilités'),\n\
                   ('s3','Créances diverses'),\n\
                   ('s4','AT');";
        let stmts = split_statements(sql);
        assert_eq!(stmts.len(), 1, "expected one statement, got {stmts:?}");
        let stmt = stmts.first().copied().unwrap_or_default();
        // The full statement must survive — including the final row + closing paren.
        assert!(stmt.ends_with("('s4','AT')"), "statement truncated: {stmt}");
        assert!(stmt.contains("Réserve légale"));
        assert!(stmt.contains("Créances diverses"));
    }

    /// Multiple statements separated by `;`, each carrying multi-byte content,
    /// must split at the correct byte boundaries.
    #[test]
    fn multibyte_multiple_statements() {
        let sql = "INSERT INTO t VALUES ('é');\nINSERT INTO t VALUES ('à');";
        let stmts = split_statements(sql);
        assert_eq!(stmts.len(), 2, "got {stmts:?}");
        assert!(stmts.first().copied().unwrap_or_default().contains("'é'"));
        assert!(stmts.get(1).copied().unwrap_or_default().contains("'à'"));
    }

    /// Escaped quotes (`''`) inside a multi-byte string literal must not end
    /// the string early or desynchronize byte offsets.
    #[test]
    fn escaped_quotes_with_multibyte() {
        let sql = "INSERT INTO t VALUES ('Cré''ance'),('x');";
        let stmts = split_statements(sql);
        assert_eq!(stmts.len(), 1, "got {stmts:?}");
        assert!(stmts.first().copied().unwrap_or_default().contains("Cré''ance"));
    }

    /// A `;` inside a string literal (with multi-byte chars present) must NOT
    /// split the statement.
    #[test]
    fn semicolon_inside_multibyte_string() {
        let sql = "INSERT INTO t VALUES ('café; thé');";
        let stmts = split_statements(sql);
        assert_eq!(stmts.len(), 1, "got {stmts:?}");
        assert!(stmts.first().copied().unwrap_or_default().contains("café; thé"));
    }

    /// Pure-ASCII multi-statement splitting still works (no regression).
    #[test]
    fn ascii_multi_statement() {
        let sql = "CREATE TABLE t (a TEXT); INSERT INTO t VALUES ('x'); SELECT * FROM t;";
        let stmts = split_statements(sql);
        assert_eq!(stmts.len(), 3, "got {stmts:?}");
    }

    /// Classification stays correct after the rewrite.
    #[test]
    fn classify_basic() {
        assert_eq!(classify("SELECT 1"), SqlKind::Select);
        assert_eq!(classify("  -- c\nINSERT INTO t VALUES (1)"), SqlKind::Dml);
        assert_eq!(classify("CREATE TABLE t (a)"), SqlKind::Ddl);
        assert_eq!(classify("WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"), SqlKind::Dml);
        assert_eq!(classify("WITH x AS (SELECT 1) SELECT * FROM x"), SqlKind::Select);
    }
}
