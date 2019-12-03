#![doc(html_root_url = "https://docs.rs/tower-limit/0.3.0-alpha.2")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]

//! Tower middleware for limiting requests.

// pub mod concurrency;
pub mod rate;

pub use crate::rate::{RateLimit, RateLimitLayer};

// concurrency::{ConcurrencyLimit, ConcurrencyLimitLayer},
