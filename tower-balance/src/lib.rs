//! Load balancing middlewares.

#![doc(html_root_url = "https://docs.rs/tower-balance/0.1.0")]
#![deny(missing_docs)]
#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]
// #![deny(warnings)]
// TODO: remove this, I need to add this due to a bug in pin-project
// sending out a false positive and I am unable to add this allow directly
// to the item due to macro magic.
#![allow(dead_code)]

pub mod error;
pub mod p2c;
// pub mod pool;
