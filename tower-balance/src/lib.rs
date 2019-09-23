//! Load balancing middlewares.

#![doc(html_root_url = "https://docs.rs/tower-balance/0.3.0-alpha.1")]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]
#![allow(elided_lifetimes_in_paths)]

pub mod error;
pub mod p2c;
pub mod pool;
