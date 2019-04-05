//! Error types

use std::fmt;

#[derive(Debug)]
pub enum Error {
    TimerError(tokio_timer::Error),
    ServiceError(::Error),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::TimerError(e) => Some(e),
            Error::ServiceError(e) => Some(&**e),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::TimerError(e) => write!(f, "timer error in delay: {:?}", e),
            Error::ServiceError(e) => write!(f, "service error in delay: {:?}", e),
        }
    }
}
