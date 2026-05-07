//! File content splitter infrastructure.
//!
//! Splits source files into semantic chunks for indexing.
//! Uses a chain-of-responsibility pattern: the first splitter
//! that supports a given file extension handles it.

pub(crate) mod fixed_size;
pub(crate) mod tree_sitter;

use std::path::Path;

use crate::types::Chunk;

/// Trait for file content splitters.
///
/// Each implementation handles a subset of file extensions and
/// produces [`Chunk`]s from file content.
pub(crate) trait Splitter: Send + Sync {
    /// Check if this splitter handles the given file extension.
    fn supports(&self, extension: &str) -> bool;

    /// Split file content into chunks.
    ///
    /// `path` is provided for context (extension detection, chunk naming)
    /// but the content is already read and passed as `content`.
    fn split(&self, content: &str, path: &Path) -> Vec<Chunk>;
}

/// Chain of splitters, tried in order.
///
/// The first splitter whose [`Splitter::supports`] returns `true`
/// handles the file.  If none match, the fallback (last in chain)
/// handles everything.
pub(crate) struct SplitterChain {
    /// Ordered list of splitters.  Last entry should be a catch-all fallback.
    splitters: Vec<Box<dyn Splitter>>,
}

impl SplitterChain {
    /// Create a new chain with the default splitter set.
    ///
    /// Currently: fixed-size fallback only.
    /// Tree-sitter AST splitter will be added in a later phase.
    pub(crate) fn new() -> Self {
        Self {
            splitters: vec![
                // Tree-sitter AST-based splitter for supported languages.
                // Extracts semantic units (functions, structs, classes, etc.).
                Box::new(tree_sitter::TreeSitterSplitter::new()),
                // Fallback: fixed-size character-based splitting.
                // Must be last — `supports()` returns `true` for all extensions.
                Box::new(fixed_size::FixedSizeSplitter::new()),
            ],
        }
    }

    /// Split file content using the first matching splitter in the chain.
    pub(crate) fn split(&self, content: &str, path: &Path) -> Vec<Chunk> {
        let ext = path.extension().and_then(std::ffi::OsStr::to_str).unwrap_or("");

        for splitter in &self.splitters {
            if splitter.supports(ext) {
                return splitter.split(content, path);
            }
        }

        // Should never reach here if chain has a catch-all fallback
        Vec::new()
    }
}
