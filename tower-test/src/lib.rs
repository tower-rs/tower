#![doc(html_root_url = "https://docs.rs/tower-test/0.3.0-alpha.1")]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]
#![allow(elided_lifetimes_in_paths)]

//! Mock `Service` that can be used in tests.

mod macros;
pub mod mock;
