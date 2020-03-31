//! Error types

use std::fmt;

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

/// An error returned when the balancer's endpoint discovery stream fails.
#[derive(Debug)]
pub struct Discover(pub(crate) Error);

impl fmt::Display for Discover {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "load balancer discovery error: {}", self.0)
    }
}

impl std::error::Error for Discover {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}
