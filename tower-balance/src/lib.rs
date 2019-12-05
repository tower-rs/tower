//! Load balancing middlewares.

#![doc(html_root_url = "https://docs.rs/tower-balance/0.3.0")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]

pub mod error;
pub mod p2c;
pub mod pool;
