//! Limit the max number of requests being concurrently processed.

pub mod future;
mod layer;
mod never;
mod service;

pub use self::{layer::ConcurrencyLimitLayer, service::ConcurrencyLimit};

type Error = Box<dyn std::error::Error + Send + Sync>;
