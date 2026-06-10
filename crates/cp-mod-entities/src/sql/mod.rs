//! SQL preprocessing and error enrichment.
//!
//! Groups the statement parsing helpers (classification + splitting) with the
//! error-enrichment layer that decorates `SQLite` errors with schema context.

/// SQL error enrichment: fuzzy suggestions and schema context.
pub(crate) mod errors;
/// SQL statement parsing: classification + UTF-8-safe statement splitting.
pub(crate) mod parse;
