//! Error types

use std::fmt;
use std::sync::Arc;

/// An error produced by a `Service` wrapped by a `Buffer`
#[derive(Debug)]
pub struct ServiceError<E> {
    method: &'static str,
    inner: E,
}

/// Error produced when spawning the worker fails
#[derive(Debug)]
pub struct SpawnError<T> {
    inner: T,
}

/// Errors produced by `Buffer`.
#[derive(Debug)]
pub enum Error<E> {
    /// The `Service` call errored.
    Inner(E),
    /// The underlying `Service` failed. All subsequent requests will fail.
    Closed(Arc<ServiceError<E>>),
    /// The underlying `Service` is currently at capacity; wait for `poll_ready`.
    Full,
}

// ===== impl Error =====

impl<T> fmt::Display for Error<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Inner(ref why) => fmt::Display::fmt(why, f),
            Error::Closed(ref e) => write!(f, "Service::{} failed: {}", e.method, e.inner),
            Error::Full => f.pad("Service at capacity"),
        }
    }
}

impl<T> std::error::Error for Error<T>
where
    T: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::Inner(ref why) => Some(why),
            Error::Closed(ref e) => Some(&e.inner),
            Error::Full => None,
        }
    }
}

// ===== impl ServiceError =====

impl<E> ServiceError<E> {
    pub(crate) fn new(method: &'static str, inner: E) -> ServiceError<E> {
        ServiceError { method, inner }
    }

    /// The error produced by the `Service` when `method` was called.
    pub fn error(&self) -> &E {
        &self.inner
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
