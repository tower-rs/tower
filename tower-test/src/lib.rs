#![doc(html_root_url = "https://docs.rs/tower-test/0.3.0-alpha.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![cfg_attr(test, deny(warnings))]
#![allow(elided_lifetimes_in_paths)]

//! Mock `Service` that can be used in tests.

mod macros;
pub mod mock;
