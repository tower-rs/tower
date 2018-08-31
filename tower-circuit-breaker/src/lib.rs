#![deny(missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]

extern crate futures;
extern crate rand;
extern crate tokio_timer;
#[cfg(test)]
extern crate tokio_executor;

mod ema;
pub mod backoff;
pub mod failure_accrual;

#[cfg(test)]
mod mock_clock;

use std::time::{Instant, Duration};
use std::fmt::{self, Display};

use tokio_timer::clock;

pub use self::backoff::Backoff;
pub use self::failure_accrual::FailureAccrualPolicy;

/// States of the circuit breaker's state machine.
#[derive(Clone, Debug)]
pub enum State {
    /// A closed breaker is operating normally and allowing.
    Closed,
    /// An open breaker has tripped and will not allow requests through until an interval expired.
    Open(Instant, Duration),
    /// A half open breaker has completed its wait interval and will allow requests. The state keeps
    /// the previous duration in an open state.
    HalfOpen(Duration),
}

/// A circuit breaker manages the state of a backend system. The circuit breaker is implemented via
/// a finite state machine with three states: `Closed`, `Open` and `HalfOpen`. The circuit breaker
/// does not know anything about the backend's state by itself, but uses the information provided
/// by the method via `on_success` and `on_error` events. Before communicating with the backend,
/// the the permission to do so must be obtained via the method `is_call_permitted`.
///
/// The state of the circuit breaker changes from `Closed` to `Open` when the `FailureAccrualPolicy`
/// reports that the failure rate is above a (configurable) threshold. Then, all access to the backend
/// is blocked for a time duration provided by `FailureAccrualPolicy`.
///
/// After the time duration has elapsed, the circuit breaker state changes from `Open` to `HalfOpen`
/// and allows calls to see if the backend is still unavailable or has become available again.
/// If the circuit breaker receives a failure on the next call, the state will change back to `Open`.
/// Otherwise it changes back to `Closed`.
///
/// # Example
///
/// ```
/// # extern crate tower_circuit_breaker;
/// # use tower_circuit_breaker::{CircuitBreaker, Instrumentation, State};
///
/// let mut circuit_breaker = CircuitBreaker::default();
///
/// // perform a success request.
/// assert!(circuit_breaker.is_call_permitted());
/// circuit_breaker.on_success();
///
/// // perform a failed request.
/// assert!(circuit_breaker.is_call_permitted());
/// circuit_breaker.on_error();
/// ```
#[derive(Debug)]
pub struct CircuitBreaker<P, I> {
    policy: P,
    instrumentation: I,
    state: State,
}

/// Consumes the circuit breaker events. May used for metrics and/or logs.
///
///
/// # Example
///
/// ```
/// # extern crate tower_circuit_breaker;
/// # use tower_circuit_breaker::{CircuitBreaker, Instrumentation, State};
///
/// struct Log;
///
/// impl Instrumentation for Log {
///   fn on_transition(&self, state: &State) {
///     println!("circuit breaker now in '{}' state", state);
///   }
///
///   fn on_call_rejected(&self) {
///     eprintln!("call rejected");
///   }
/// }
///
/// let _ = CircuitBreaker::default().with_instrumentation(Log);
///
/// ```
pub trait Instrumentation {
    fn on_transition(&self, state: &State);
    fn on_call_rejected(&self);
}

/// An instrumentation which does noting.
#[derive(Debug)]
pub struct NoopInstrumentation;

impl Instrumentation for NoopInstrumentation {
    fn on_transition(&self, _state: &State) {}

    fn on_call_rejected(&self) {}
}

impl State {
    /// Returns a string value for the state identifier.
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Open(_, _) => "open",
            State::Closed => "closed",
            State::HalfOpen(_) => "half_open",
        }
    }

    /// `true` if state is `Open`.
    pub fn is_open(&self) -> bool {
        if let State::Open(_, _) = self {
            true
        } else {
            false
        }
    }

    /// `true` if state is `HalfOpen`.
    pub fn is_half_open(&self) -> bool {
        if let State::HalfOpen(_) = self {
            true
        } else {
            false
        }
    }

    /// `true` if state is `Closed`.
    pub fn is_closed(&self) -> bool {
        if let State::Closed = self {
            true
        } else {
            false
        }
    }
}

impl Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.as_str())
    }
}

impl<P> CircuitBreaker<P, NoopInstrumentation> {
    /// Creates a new circuit breaker with given failure policy and `NoopInstrumentation`.
    pub fn new(policy: P) -> Self {
        CircuitBreaker {
            policy,
            instrumentation: NoopInstrumentation,
            state: State::Closed,
        }
    }
}

impl<P, I> CircuitBreaker<P, I>
where
    P: FailureAccrualPolicy,
    I: Instrumentation
{
    /// Add an instrumentation for this circuit breaker.
    pub fn with_instrumentation<T>(self, instrumentation: T) -> CircuitBreaker<P, T>
    where
        T: Instrumentation
    {
        CircuitBreaker {
            instrumentation,
            policy: self.policy,
            state: self.state,
        }
    }

    /// Add a failure policy for this circuit breaker.
    pub fn with_policy<T>(self, policy: T) -> CircuitBreaker<T, I>
    where
        T: FailureAccrualPolicy
    {
        CircuitBreaker {
            policy,
            instrumentation: self.instrumentation,
            state: self.state,
        }
    }

    /// Requests permission to call this circuit breaker's backend.
    pub fn is_call_permitted(&mut self) -> bool {
        match self.state {
            State::Closed => true,
            State::HalfOpen(_) => true,
            State::Open(until, delay) => {
                if clock::now() > until {
                    self.transit_to_half_open(delay);
                    return true;
                }
                self.instrumentation.on_call_rejected();
                false
            }
        }
    }

    /// Records a successful call.
    ///
    /// This method must be invoked when a call was success.
    pub fn on_success(&mut self) {
        if let State::HalfOpen(_) = self.state {
            self.reset();
        }
        self.policy.record_success()
    }

    /// Records a failed call.
    ///
    /// This method must be invoked when a call failed.
    pub fn on_error(&mut self) {
        match self.state {
            State::Closed => {
                if let Some(delay) = self.policy.mark_dead_on_failure() {
                    self.transit_to_open(delay);
                }
            }
            State::HalfOpen(delay_in_half_open) => {
                // Pick up the next open state's delay from the policy, if policy returns Some(_)
                // use it, otherwise reuse the delay from the current state.
                let delay = self.policy.mark_dead_on_failure().unwrap_or(delay_in_half_open);
                self.transit_to_open(delay);
            }
            _ => {}
        }
    }

    /// Returns the circuit breaker to its original closed state, losing statistics.
    #[inline]
    pub fn reset(&mut self) {
        self.state = State::Closed;
        self.policy.revived();
        self.instrumentation.on_transition(&self.state);
    }

    #[inline]
    fn transit_to_half_open(&mut self, delay: Duration) {
        self.state = State::HalfOpen(delay);
        self.instrumentation.on_transition(&self.state);
    }

    #[inline]
    fn transit_to_open(&mut self, delay: Duration) {
        let until = clock::now() + delay;
        self.state = State::Open(until, delay);
        self.instrumentation.on_transition(&self.state);
    }
}

impl Default for CircuitBreaker<
    failure_accrual::OrElse<
        failure_accrual::SuccessRate<backoff::EqualJittered>,
        failure_accrual::ConsecutiveFailures<backoff::EqualJittered>
    >,
    NoopInstrumentation
>
{
    /// Creates a new circuit breaker instance which configured with default
    /// backoff strategy and failure accrual policy.
    fn default() -> Self {
        let policy = failure_accrual::SuccessRate::default()
                .or_else(failure_accrual::ConsecutiveFailures::default());
        CircuitBreaker::new(policy)
    }
}

