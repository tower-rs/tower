use std::error::Error as StdError;
use std::fmt::{self, Display};

/// A `CircuitBreaker`'s error.
#[derive(Debug)]
pub enum Error<E> {
    /// An error from inner call.
    Upstream(E),
    /// An error when call was rejected.
    Rejected,
}

impl<E> Display for Error<E>
where
    E: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Rejected => write!(f, "call was rejected"),
            Error::Upstream(err) => write!(f, "{}", err),
        }
    }
}

impl<E> StdError for Error<E>
where
    E: StdError,
{
    fn description(&self) -> &str {
        match self {
            Error::Rejected => "call was rejected",
            Error::Upstream(err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match self {
            Error::Upstream(ref err) => Some(err),
            _ => None,
        }
    }
}
