//! Error types

use std::fmt;

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

/// An error returned by `Overload` when the underlying service
/// is not ready to handle any requests at the time of being
/// called.
pub struct Overloaded {
    _p: (),
}

impl Overloaded {
    pub(crate) fn new() -> Self {
        Overloaded { _p: () }
    }
}

impl fmt::Debug for Overloaded {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Overloaded")
    }
}

impl fmt::Display for Overloaded {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("service overloaded")
    }
}

impl std::error::Error for Overloaded {}
