//! Error types

use std::{error, fmt};

pub(crate) type Error = Box<error::Error + Send + Sync>;

/// The timeout elapsed.
#[derive(Debug)]
pub struct Elapsed(pub(super) ());

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("request timed out")
    }
}

impl error::Error for Elapsed {}
