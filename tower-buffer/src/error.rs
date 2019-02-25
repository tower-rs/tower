//! Error types

use std::fmt;
use std::sync::Arc;

/// An error produced by a `Service` wrapped by a `Buffer`
#[derive(Debug)]
pub struct ServiceError {
    method: &'static str,
    inner: Arc<Error>,
}

/// Error produced when spawning the worker fails
#[derive(Debug)]
pub struct SpawnError<T> {
    inner: T,
}

/// Errors produced by `Buffer`.
pub(crate) type Error = Box<::std::error::Error + Send + Sync>;

// ===== impl ServiceError =====

impl ServiceError {
    pub(crate) fn new(method: &'static str, inner: Error) -> ServiceError {
        let inner = Arc::new(inner);
        ServiceError { method, inner }
    }

    /// Private to avoid exposing `Clone` trait as part of the public API
    pub(crate) fn clone(&self) -> ServiceError {
        ServiceError {
            method: self.method,
            inner: self.inner.clone(),
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Service::{} failed: {}", self.method, self.inner)
    }
}

impl std::error::Error for ServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&**self.inner)
    }
}

// ===== impl SpawnError =====

impl<T> SpawnError<T> {
    pub(crate) fn new(inner: T) -> SpawnError<T> {
        SpawnError { inner }
    }
}

impl<T> fmt::Display for SpawnError<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error spawning buffer task: {:?}", self.inner)
    }
}

impl<T> std::error::Error for SpawnError<T>
where
    T: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}
