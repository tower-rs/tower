//! Error types

use std::fmt;

/// An error returned by [`LoadShed`] when the underlying service
/// is not ready to handle any requests at the time of being
/// called.
///
/// [`LoadShed`]: crate::load_shed::LoadShed
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
