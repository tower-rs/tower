//! Error types

use std::{error, fmt};

/// Error produced by `Filter`
#[derive(Debug)]
pub struct Error {
    source: Option<Source>,
}

pub(crate) type Source = Box<dyn error::Error + Send + Sync>;

impl Error {
    /// Create a new `Error` representing a rejected request.
    pub fn rejected() -> Error {
        Error { source: None }
    }

    /// Create a new `Error` representing an inner service error.
    pub fn inner<E>(source: E) -> Error
    where
        E: Into<Source>,
    {
        Error {
            source: Some(source.into()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        if self.source.is_some() {
            write!(fmt, "inner service errored")
        } else {
            write!(fmt, "rejected")
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        if let Some(ref err) = self.source {
            Some(&**err)
        } else {
            None
        }
    }
}
