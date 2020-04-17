//! Error types for the `Buffer` middleware.

use std::{fmt, sync::Arc};

/// An error produced by a `Service` wrapped by a `Buffer`
#[derive(Debug)]
pub struct ServiceError {
    inner: Arc<Error>,
}

/// An error produced when the a buffer's worker closes unexpectedly.
pub struct Closed {
    _p: (),
}

/// Errors produced by `Buffer`.
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

// ===== impl ServiceError =====

impl ServiceError {
    pub(crate) fn new(inner: Error) -> ServiceError {
        let inner = Arc::new(inner);
        ServiceError { inner }
    }

    // Private to avoid exposing `Clone` trait as part of the public API
    pub(crate) fn clone(&self) -> ServiceError {
        ServiceError {
            inner: self.inner.clone(),
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "buffered service failed: {}", self.inner)
    }
}

impl std::error::Error for ServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&**self.inner)
    }
}

// ===== impl Closed =====

impl Closed {
    pub(crate) fn new() -> Self {
        Closed { _p: () }
    }
}

impl fmt::Debug for Closed {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_tuple("Closed").finish()
    }
}

impl fmt::Display for Closed {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str("buffer's worker closed unexpectedly")
    }
}

impl std::error::Error for Closed {}
