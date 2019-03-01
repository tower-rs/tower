//! Error types

use std::error;
use std::fmt;

/// Error produced by `Filter`
#[derive(Debug)]
pub struct Error {
    reason: Reason,
    source: Option<Source>,
}

#[derive(Debug)]
enum Reason {
    Rejected,
    Inner,
}

pub(crate) type Source = Box<dyn error::Error + Send + Sync>;

impl Error {
    /// Create a new `Error` representing a rejected request.
    pub fn rejected<E>(source: Option<E>) -> Error
    where
        E: Into<Source>,
    {
        Error {
            reason: Reason::Rejected,
            source: source.map(Into::into),
        }
    }

    /// Create a new `Error` representing an inner service error.
    pub fn inner<E>(source: E) -> Error
    where
        E: Into<Source>,
    {
        Error {
            reason: Reason::Inner,
            source: Some(source.into()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self.reason {
            Reason::Rejected => {
                if let Some(source) = &self.source {
                    write!(fmt, "rejected: {}", source)
                } else {
                    write!(fmt, "rejected")
                }
            }
            Reason::Inner => {
                if let Some(source) = &self.source {
                    source.fmt(fmt)
                } else {
                    write!(fmt, "inner service error")
                }
            }
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

pub(crate) mod never {
    #[derive(Debug)]
    pub enum Never {}
}
