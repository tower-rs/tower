//! Error types

use std::fmt;
use tokio_executor;

/// Error produced when spawning the worker fails
#[derive(Debug)]
pub struct SpawnError {
    inner: tokio_executor::SpawnError,
}

/// Errors produced by `SpawnReady`.
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

// ===== impl SpawnError =====

impl SpawnError {
    pub(crate) fn new(inner: tokio_executor::SpawnError) -> Self {
        Self { inner }
    }
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl std::error::Error for SpawnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}
