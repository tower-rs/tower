//! Error types

use std::{error, fmt};

pub(crate) type Error = Box<dyn error::Error + Send + Sync>;

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
