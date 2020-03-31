//! Limit the rate at which requests are processed.

mod layer;
mod rate;
mod service;

pub use self::{layer::RateLimitLayer, rate::Rate, service::RateLimit};
