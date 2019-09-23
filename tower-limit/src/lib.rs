#![doc(html_root_url = "https://docs.rs/tower-limit/0.3.0-alpha.1")]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]
#![allow(elided_lifetimes_in_paths)]

//! Tower middleware for limiting requests.

pub mod concurrency;
pub mod rate;

pub use crate::{
    concurrency::{ConcurrencyLimit, ConcurrencyLimitLayer},
    rate::{RateLimit, RateLimitLayer},
};
