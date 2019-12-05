//! A cache of services

#![doc(html_root_url = "https://docs.rs/tower-ready-cache/0.1.0")]
#![deny(missing_docs)]
#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

pub mod cache;
pub mod error;

pub use self::cache::ReadyCache;
