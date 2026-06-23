//! Circuit breaker middleware for Tower services.
//!
//! Prevents cascading failures by tracking service health and
//! short-circuiting requests to a failing backend before they hit the
//! network.
//!
//! # States
//!
//! ```text
//! Closed ──(N consecutive failures)──► Open
//! Open   ──(timeout elapsed)─────────► HalfOpen  (one probe allowed)
//! HalfOpen ──(success rate ≥ threshold)► Closed
//! HalfOpen ──(probe fails)────────────► Open
//! ```
//!
//! - **Closed** — normal operation; all requests pass through.
//! - **Open** — service is unhealthy; requests are rejected immediately
//!   with [`CircuitError::Open`], avoiding latency pile-up.
//! - **Half-Open** — after the recovery timeout elapses, one probe request
//!   is allowed through.  On success the circuit closes; on failure it
//!   reopens.
//!
//! # Policies
//!
//! The circuit-breaking logic is separated from the state machine via the
//! [`CircuitPolicy`] trait.  The built-in [`ConsecutiveFailures`] policy
//! opens after *N* consecutive failures and closes once enough probes
//! succeed.  Implement [`CircuitPolicy`] directly to build latency-based
//! triggers, manual switches, or any other strategy.
//!
//! # Relationship to [`tower::retry::budget`]
//!
//! [`Budget`][budget] and circuit breakers are **complementary**, not
//! competing.
//!
//! - A **retry budget** governs *retry worthiness*: it limits how many
//!   retried requests can be issued relative to the originals, preventing
//!   retry amplification inside a single client.
//! - A **circuit breaker** governs *traffic admission*: once failure is
//!   systemic it stops **all** requests (including first attempts) from
//!   reaching the backend, giving it breathing room to recover.
//!
//! Using a circuit breaker without a budget still exposes you to retry
//! storms from clients above; using a budget without a circuit breaker
//! still allows traffic to pile up against a failing backend.  The two
//! compose naturally:
//!
//! ```rust,ignore
//! use std::{future, sync::Arc, time::Duration};
//! use tower::{ServiceBuilder, retry::{Policy, budget::TpsBudget}};
//! use tower::circuit_breaker::CircuitBreakerLayer;
//!
//! // Budget caps how many retries each client issues.
//! // Circuit breaker stops all traffic once failure is systemic.
//! let svc = ServiceBuilder::new()
//!     .layer(CircuitBreakerLayer::new(5, 0.8, Duration::from_secs(30)))
//!     .layer(tower::retry::RetryLayer::new(my_budget_policy))
//!     .service_fn(my_backend);
//! ```
//!
//! [budget]: crate::retry::budget
//!
//! # Quick start
//!
//! ```rust,ignore
//! use std::time::Duration;
//! use tower::ServiceBuilder;
//! use tower::circuit_breaker::CircuitBreakerLayer;
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

mod future;
mod layer;
mod policy;
mod service;

pub use self::{
    future::ResponseFuture,
    layer::CircuitBreakerLayer,
    policy::{CircuitPolicy, ConsecutiveFailures},
    service::{CircuitBreaker, CircuitError, CircuitStatus},
};
