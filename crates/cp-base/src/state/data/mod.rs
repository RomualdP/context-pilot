//! Serializable data structures for persistence (`config::Shared`, `WorkerState`, `Message`).

/// Persistence structs: `config::Shared`, `WorkerState`, `PanelData`.
pub mod config;
/// Message struct and conversation formatting.
pub mod message;
/// Model selection, pricing, and cleaning-threshold helpers for [`super::runtime::State`].
pub(crate) mod model_helpers;
