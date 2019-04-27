#![doc(html_root_url = "https://docs.rs/tower-map/0.1.0")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![allow(elided_lifetimes_in_paths)]

//! Tower middleware for converting the request type

mod layer;
mod service;
mod error;

pub use error::Error;
pub use self::{layer::MapRequestLayer, service::MapRequest};

/// A mapping implemented by a closure
pub fn mapping_fn<T>(f: T) -> MapRequestLayer<T> {
  MapRequestLayer::new(f)
}
