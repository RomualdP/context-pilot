//! Meilisearch HTTP client and server lifecycle management.
//!
//! Groups the Meilisearch-specific code: the HTTP API client, binary
//! download logic, and server start/stop/health lifecycle.

/// HTTP API client for Meilisearch: index management, document CRUD, search.
pub(crate) mod client;
/// Binary download and platform detection.
pub(crate) mod download;
/// Server lifecycle: start, stop, health check, reconnect.
pub(crate) mod server;
