#![deny(missing_debug_implementations)]
//#![cfg_attr(test, deny(warnings))]

extern crate futures;
extern crate rand;
extern crate spin;
extern crate tokio_timer;

#[cfg(test)]
extern crate tokio_executor;

mod ema;
mod instrument;
mod state_machine;
mod windowed_adder;

pub mod backoff;
pub mod failure_policy;
pub mod failure_predicate;

#[cfg(test)]
mod mock_clock;

pub use backoff::Backoff;
pub use failure_policy::{
    consecutive_failures, success_rate_over_time_window, ConsecutiveFailures, FailurePolicy,
    SuccessRateOverTimeWindow,
};
pub use instrument::Instrument;
pub use state_machine::StateMachine;
