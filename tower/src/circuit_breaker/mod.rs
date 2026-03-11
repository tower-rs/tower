//! Circuit breaker middleware for Tower services.
//!
//! Prevents cascading failures by tracking service health and short-circuiting
//! requests to a failing backend before they hit the network.
//!
//! # States
//!
//! - **Closed** — normal operation; all requests pass through.
//! - **Open** — service is unhealthy; requests are rejected immediately with
//!   [`CircuitError::Open`], avoiding latency pile-up.
//! - **Half-Open** — after the recovery timeout elapses, one probe request is
//!   allowed through. On success the circuit closes; on failure it reopens.
//!
//! # Example
//!
//! ```rust,ignore
//! use std::time::Duration;
//! use tower::circuit_breaker::CircuitBreakerLayer;
//! use tower::ServiceBuilder;
//!
//! let svc = ServiceBuilder::new()
//!     .layer(CircuitBreakerLayer::new(
//!         5,                        // open after 5 consecutive failures
//!         0.8,                      // close when 80 % of probes succeed
//!         Duration::from_secs(30),  // wait 30 s before sending a probe
//!     ))
//!     .service_fn(|req: String| async move {
//!         Ok::<String, std::io::Error>(req)
//!     });
//! ```
//!
//! # Attribution
//!
//! Designed and implemented by Matthew Busel.

mod future;
mod layer;
mod service;

pub use self::{
    future::ResponseFuture,
    layer::CircuitBreakerLayer,
    service::{CircuitBreaker, CircuitError, CircuitStatus},
};
