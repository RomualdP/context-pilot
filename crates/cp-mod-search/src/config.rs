//! Configuration constants for the search module.
//!
//! Extension allowlists, path exclusions, and size limits.
//! Hardcoded defaults can be overridden via `.context-pilot/search.toml`.

/// Maximum file size in bytes (1 MB).
///
/// Files larger than this are skipped during indexing to avoid
/// overwhelming the search index with very large generated files.
pub(crate) const MAX_FILE_SIZE: u64 = 0x0010_0000;

/// Default chunk size in characters for the fixed-size fallback splitter.
pub(crate) const FALLBACK_CHUNK_SIZE: usize = 4000;

/// Extensions that are eligible for indexing (code, config, docs, web, build).
///
/// Returns `true` if the extension is in the hardcoded allowlist.
pub(crate) fn is_allowed_extension(ext: &str) -> bool {
    matches!(
        ext,
        // Code
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx"
            | "go" | "java" | "c" | "h" | "cpp" | "hpp" | "cc"
            | "rb" | "php" | "swift" | "kt" | "scala"
            | "ex" | "exs" | "hs" | "ml" | "lua" | "dart"
            | "zig" | "nix" | "tf" | "sh" | "bash" | "zsh"
            | "sql" | "cs" | "fs" | "vb" | "pl" | "pm"
            | "r" | "jl" | "nim" | "sol" | "v" | "vy" | "move"
        // Config / data
        | "toml" | "yaml" | "yml" | "json" | "xml"
            | "ini" | "cfg" | "conf" | "properties"
        // Documentation
        | "md" | "txt" | "rst" | "adoc" | "org" | "tex"
        // Web
        | "html" | "htm" | "css" | "scss" | "sass" | "less" | "svg"
        // Build
        | "dockerfile" | "makefile" | "cmake" | "gradle" | "sbt"
        // Other
        | "graphql" | "proto" | "thrift"
    )
}

/// Directory names that are always skipped during indexing.
const EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "vendor",
    "target",
    "dist",
    "build",
    "out",
    "__pycache__",
    ".next",
    ".nuxt",
    ".context-pilot",
];

/// File patterns (suffixes) that are always skipped during indexing.
const EXCLUDED_SUFFIXES: &[&str] = &[".min.js", ".min.css", ".map", ".lock", ".sum"];

/// Check if a path component is an excluded directory.
pub(crate) fn is_excluded_dir(name: &str) -> bool {
    EXCLUDED_DIRS.contains(&name)
}

/// Check if a filename matches an excluded suffix pattern.
pub(crate) fn is_excluded_file(filename: &str) -> bool {
    EXCLUDED_SUFFIXES.iter().any(|suffix| filename.ends_with(suffix))
}

/// Meilisearch settings for the **files** index.
///
/// Defines which fields are searchable, filterable, and sortable.
/// See design doc §4 "Files Index" for rationale.
pub(crate) fn files_index_settings() -> serde_json::Value {
    serde_json::json!({
        "searchableAttributes": ["content", "chunk_name", "file_path"],
        "filterableAttributes": ["file_path", "extension", "chunk_type"],
        "sortableAttributes": ["last_modified_ms"],
        "typoTolerance": {
            "enabled": true,
            "minWordSizeForTypos": { "oneTypo": 4, "twoTypos": 8 }
        }
    })
}

/// Meilisearch settings for the **logs** index.
///
/// Defines which fields are searchable, filterable, and sortable.
/// See design doc §4 "Logs Index" for rationale.
pub(crate) fn logs_index_settings() -> serde_json::Value {
    serde_json::json!({
        "searchableAttributes": ["content", "tags"],
        "filterableAttributes": ["timestamp_ms", "importance", "tags", "worker_id"],
        "sortableAttributes": ["timestamp_ms"]
    })
}
