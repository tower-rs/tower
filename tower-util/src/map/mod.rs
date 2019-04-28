//! Tower middleware for synchronously converting the request type

mod layer;
mod service;

pub use self::{layer::MapRequestLayer, service::MapRequest};

/// A mapping implemented by a closure
pub fn mapping_fn<T>(f: T) -> MapRequestLayer<T> {
  MapRequestLayer::new(f)
}
