#![doc(html_root_url = "https://docs.rs/tower-test/0.4.0")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]
// `rustdoc::broken_intra_doc_links` is checked on CI

//! Mock `Service` that can be used in tests.

mod macros;
pub mod mock;
