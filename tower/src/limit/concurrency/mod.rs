//! Limit the max number of requests being concurrently processed.

pub mod future;
mod layer;
mod service;
mod sync;

pub use self::{layer::ConcurrencyLimitLayer, service::ConcurrencyLimit};
