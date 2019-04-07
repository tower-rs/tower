#![deny(rust_2018_idioms)]
//! Limit inbound requests.

pub mod concurrency;
pub mod rate;

pub use crate::{
    concurrency::{ConcurrencyLimit, ConcurrencyLimitLayer},
    rate::{RateLimit, RateLimitLayer},
};
