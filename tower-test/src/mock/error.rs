//! Error types

use std::error;
use std::fmt;

pub(crate) type Error = Box<error::Error + Send + Sync>;

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
