//! Error types

use std::fmt;

/// Error produced when spawning the worker fails
#[derive(Debug)]
pub struct SpawnError {
    _p: (),
}

/// Errors produced by `SpawnReady`.
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

// ===== impl SpawnError =====

impl SpawnError {
    pub(crate) fn new() -> SpawnError {
        SpawnError { _p: () }
    }
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "failed to spawn BackgroundReady task")
    }
}

impl std::error::Error for SpawnError {}
