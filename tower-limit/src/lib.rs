//! Limit inbound requests.

#[macro_use]
extern crate futures;





pub mod concurrency;
pub mod rate;

pub use crate::concurrency::{ConcurrencyLimit, ConcurrencyLimitLayer};
pub use crate::rate::{RateLimit, RateLimitLayer};
