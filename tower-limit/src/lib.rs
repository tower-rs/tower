#![doc(html_root_url = "https://docs.rs/tower-limit/0.1.2")]
#![cfg_attr(test, deny(warnings))]
#![deny(missing_debug_implementations, missing_docs, rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

//! Tower middleware for limiting requests.

pub mod concurrency;
pub mod rate;

pub use crate::{
    concurrency::{ConcurrencyLimit, ConcurrencyLimitLayer},
    rate::{RateLimit, RateLimitLayer},
};
