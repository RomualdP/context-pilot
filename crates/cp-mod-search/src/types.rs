//! Core types for the search module.

use serde::{Deserialize, Serialize};

/// Persisted search state — survives TUI reloads.
///
/// Serialized via `save_module_data` / `load_module_data`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SearchPersistData {
    /// TCP port the Meilisearch server is listening on.
    pub port: u16,
    /// API master key for authenticating with Meilisearch.
    pub master_key: String,
    /// 8-character hash of the project root path (for per-project index naming).
    pub project_hash: String,
    /// Whether the initial full-project indexing has completed.
    pub index_ready: bool,
}

/// Full runtime search state stored in the `State` `TypeMap`.
///
/// Contains the persisted data. Runtime-only handles (watcher, indexer
/// channel, error buffer) are added in later phases.
#[derive(Debug)]
pub(crate) struct SearchState {
    /// Persisted fields that survive TUI reloads.
    pub persist: SearchPersistData,
}
