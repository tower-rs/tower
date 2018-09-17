//#![cfg_attr(test, deny(warnings))]

#[macro_use(try_ready)]
extern crate futures;
extern crate rand;
extern crate spin;
extern crate tokio_timer;
extern crate tower_service;

#[cfg(test)]
extern crate tokio_executor;

mod ema;
mod instrument;
mod service;
mod state_machine;
mod windowed_adder;

#[cfg(test)]
mod mock_clock;

pub mod backoff;
pub mod failure_policy;
pub mod failure_predicate;

pub use backoff::Backoff;
pub use failure_policy::{
    consecutive_failures, success_rate_over_time_window, ConsecutiveFailures, FailurePolicy,
    SuccessRateOverTimeWindow,
};
pub use failure_predicate::FailurePredicate;
pub use instrument::Instrument;
pub use service::{CircuitBreakerService, Error, ResponseFuture};
pub use state_machine::StateMachine;
