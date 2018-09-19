use std::fmt::{self, Debug};
use std::time::Duration;

use futures::{Async, Future, Poll};
use spin::Mutex;
use tokio_timer::{clock, Delay, Error as TimerError};

use super::failure_policy::FailurePolicy;
use super::instrument::Instrument;

const ON_CLOSED: u8 = 0b0000_0001;
//const ON_HALF_OPEN: u8 = 0b0000_0010;
const ON_OPEN: u8 = 0b0000_1000;

/// States of the state machine.
#[derive(Debug)]
enum State {
    /// A closed breaker is operating normally and allowing.
    Closed,
    /// An open breaker has tripped and will not allow requests through until a time interval expired.
    Open(Delay, Duration),
    /// A half open breaker has completed its wait interval and will allow requests. The state keeps
    /// the previous duration in an open state.
    HalfOpen(Duration),
}

struct Shared<P> {
    state: State,
    failure_policy: P,
}

/// A circuit breaker implementation backed by state machine.
///
/// It is implemented via a finite state machine with three states: `Closed`, `Open` and `HalfOpen`.
/// The state machine does not know anything about the backend's state by itself, but uses the
/// information provided by the method via `on_success` and `on_error` events. Before communicating
/// with the backend, the the permission to do so must be obtained via the method `is_call_permitted`.
///
/// The state of the state machine changes from `Closed` to `Open` when the `FailurePolicy`
/// reports that the failure rate is above a (configurable) threshold. Then, all access to the backend
/// is blocked for a time duration provided by `FailurePolicy`.
///
/// After the time duration has elapsed, the state changes from `Open` to `HalfOpen` and allows
/// calls to see if the backend is still unavailable or has become available again. If the circuit
/// breaker receives a failure on the next call, the state will change back to `Open`. Otherwise
/// it changes to `Closed`.
pub struct StateMachine<P, I> {
    shared: Mutex<Shared<P>>,
    instrument: I,
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
}

impl<P, I> Debug for StateMachine<P, I> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let shared = self.shared.lock();
        f.debug_struct("StateMachine")
            .field("state", &(shared.state.as_str()))
            .finish()
    }
}

impl<P> Shared<P>
where
    P: FailurePolicy,
{
    #[inline]
    fn transit_to_closed(&mut self) {
        self.state = State::Closed;
        self.failure_policy.revived();
    }

    #[inline]
    fn transit_to_half_open(&mut self, delay: Duration) {
        self.state = State::HalfOpen(delay);
    }

    #[inline]
    fn transit_to_open(&mut self, delay: Duration) {
        let until = clock::now() + delay;
        let until = Delay::new(until);
        self.state = State::Open(until, delay);
    }
}

impl<P, I> StateMachine<P, I>
where
    P: FailurePolicy,
    I: Instrument,
{
    /// Creates a new state machine with given failure policy and instrument.
    pub fn new(failure_policy: P, instrument: I) -> Self {
        instrument.on_closed();

        StateMachine {
            shared: Mutex::new(Shared {
                state: State::Closed,
                failure_policy,
            }),
            instrument,
        }
    }

    /// Requests permission to call.
    ///
    /// It returns `true` if a call is allowed, or `false` if prohibited.
    pub fn is_call_permitted(&self) -> bool {
        let shared = self.shared.lock();
        if let State::Open(_, _) = shared.state {
            self.instrument.on_call_rejected();
            return false;
        }
        true
    }

    /// Wait until the circuit breaker allows a request.
    ///
    /// It returns `Ok(Async::Ready(())` when the circuit breaker permitted a call or
    /// `Ok(Async::NotReady)` when the circuit breaker in the open state.
    pub fn poll_ready(&self) -> Poll<(), TimerError> {
        let mut on_half_open: Option<Duration> = None;

        {
            let mut shared = self.shared.lock();
            if let State::Open(ref mut until, delay) = shared.state {
                let _ready = try_ready!(until.poll());
                on_half_open = Some(delay);
            }

            if let Some(delay) = on_half_open {
                shared.transit_to_half_open(delay);
            }
        };

        if on_half_open.is_some() {
            self.instrument.on_half_open();
        }

        Ok(Async::Ready(()))
    }

    /// Records a successful call.
    ///
    /// This method must be invoked when a call was success.
    pub fn on_success(&self) {
        let mut instrument: u8 = 0;
        {
            let mut shared = self.shared.lock();
            if let State::HalfOpen(_) = shared.state {
                shared.transit_to_closed();
                instrument |= ON_CLOSED;
            }
            shared.failure_policy.record_success()
        }

        if instrument & ON_CLOSED != 0 {
            self.instrument.on_closed();
        }
    }

    /// Records a failed call.
    ///
    /// This method must be invoked when a call failed.
    pub fn on_error(&self) {
        let mut instrument: u8 = 0;
        {
            let mut shared = self.shared.lock();
            match shared.state {
                State::Closed => {
                    if let Some(delay) = shared.failure_policy.mark_dead_on_failure() {
                        shared.transit_to_open(delay);
                        instrument |= ON_OPEN;
                    }
                }
                State::HalfOpen(delay_in_half_open) => {
                    // Pick up the next open state's delay from the policy, if policy returns Some(_)
                    // use it, otherwise reuse the delay from the current state.
                    let delay = shared
                        .failure_policy
                        .mark_dead_on_failure()
                        .unwrap_or(delay_in_half_open);
                    shared.transit_to_open(delay);
                    instrument |= ON_OPEN;
                }
                _ => {}
            }
        }

        if instrument & ON_OPEN != 0 {
            self.instrument.on_open();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use backoff;
    use failure_policy::consecutive_failures;
    use mock_clock::{self as clock, IntoDuration};
    use instrument::Observer;

    macro_rules! assert_ready {
        ($f:expr) => {{
            match $f.poll_ready().unwrap() {
                Async::Ready(v) => v,
                Async::NotReady => panic!("NotReady"),
            }
        }};
    }

    macro_rules! assert_not_ready {
        ($f:expr) => {{
            let res = $f.poll_ready().unwrap();
            assert!(!res.is_ready(), "actual={:?}", res)
        }};
    }

    /// Perform `Closed` -> `Open` -> `HalfOpen` -> `Open` -> `HalfOpen` -> `Closed` transitions.
    #[test]
    fn state_machine() {
        clock::freeze(move |time| {
            let observe = Observer::new();
            let backoff = backoff::exponential(5.seconds(), 300.seconds());
            let policy = consecutive_failures(3, backoff);
            let state_machine = StateMachine::new(policy, observe.clone());

            assert_ready!(state_machine);

            // Perform success requests. the circuit breaker must be closed.
            for _i in 0..10 {
                assert_eq!(true, state_machine.is_call_permitted());
                state_machine.on_success();
                assert_eq!(true, observe.is_closed());
            }

            // Perform failed requests, the circuit breaker still closed.
            for _i in 0..2 {
                assert_eq!(true, state_machine.is_call_permitted());
                state_machine.on_error();
                assert_eq!(true, observe.is_closed());
            }

            // Perform a failed request and transit to the open state for 5s.
            assert_eq!(true, state_machine.is_call_permitted());
            state_machine.on_error();
            assert_eq!(true, observe.is_open());

            // Reject call attempts, the circuit breaker in open state.
            for i in 0..10 {
                assert_eq!(false, state_machine.is_call_permitted());
                assert_eq!(i + 1, observe.rejected_calls());
            }

            // Wait 2s, the circuit breaker still open.
            time.advance(2.seconds());
            assert_not_ready!(state_machine);
            assert_eq!(true, observe.is_open());

            // Wait 4s (6s total), the circuit breaker now in the half open state.
            time.advance(4.seconds());
            assert_ready!(state_machine);
            assert_eq!(true, observe.is_half_open());

            // Perform a failed request and transit back to the open state for 10s.
            state_machine.on_error();
            assert_not_ready!(state_machine);
            assert_eq!(true, observe.is_open());

            // Wait 5s, the circuit breaker still open.
            time.advance(5.seconds());
            assert_not_ready!(state_machine);
            assert_eq!(true, observe.is_open());

            // Wait 6s (11s total), the circuit breaker now in the half open state.
            time.advance(6.seconds());
            assert_ready!(state_machine);
            assert_eq!(true, observe.is_half_open());

            // Perform a success request and transit to the closed state.
            state_machine.on_success();
            assert_ready!(state_machine);
            assert_eq!(true, observe.is_closed());

            // Perform success requests.
            for _i in 0..10 {
                assert_eq!(true, state_machine.is_call_permitted());
                state_machine.on_success();
            }
        });
    }
}
