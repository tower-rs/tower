//! Limit the max number of requests being concurrently processed.

mod future;
mod layer;
mod service;

pub use self::{future::ResponseFuture, layer::ConcurrencyLimitLayer, service::ConcurrencyLimit};

type Error = Box<dyn std::error::Error + Send + Sync>;
