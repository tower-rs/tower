//! Error types

use std::{error, fmt};

// pub(crate) type Error = Box<dyn error::Error + Send + Sync>;

/// Cloneable dyn Error
#[derive(Debug, Clone)]
pub struct Error {
    inner: std::sync::Arc<dyn std::error::Error + Send + Sync + 'static>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl<E> From<E> for Error
where
    E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    fn from(error: E) -> Self {
        let boxed = error.into();
        let inner = boxed.into();
        Self { inner }
    }
}

impl std::ops::Deref for Error {
    type Target = dyn std::error::Error + Send + Sync + 'static;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

/// Error yielded when a mocked service does not yet accept requests.
#[derive(Debug)]
pub struct Closed(());

impl Closed {
    pub(crate) fn new() -> Closed {
        Closed(())
    }
}

impl fmt::Display for Closed {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "service closed")
    }
}

impl error::Error for Closed {}
