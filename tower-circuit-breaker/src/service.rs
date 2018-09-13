use std::error::Error as StdError;
use std::fmt::{self, Display};
use std::sync::Arc;

use futures::{Async, Future, Poll};
use tokio_timer::Delay;
use tower_service::Service;

use super::failure_policy::FailurePolicy;
use super::failure_predicate::FailurePredicate;
use super::instrument::Instrument;
use super::state_machine::StateMachine;

/// A `CircuitBreaker`'s error.
#[derive(Debug)]
pub enum Error<E> {
    /// An error from inner call.
    Upstream(E),
    /// An error when call was rejected.
    Rejected,
}

impl<E> Display for Error<E>
where
    E: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Rejected => write!(f, "call was rejected"),
            Error::Upstream(err) => write!(f, "{}", err),
        }
    }
}

impl<E> StdError for Error<E>
where
    E: StdError,
{
    fn description(&self) -> &str {
        match self {
            Error::Rejected => "call was rejected",
            Error::Upstream(err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match self {
            Error::Upstream(ref err) => Some(err),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum State {
    Closed,
    Open(Delay),
}

#[derive(Debug)]
struct Shared<P, I, T> {
    state_machine: StateMachine<P, I>,
    failure_predicate: T,
}

/// Circuit breaker services, it wraps an inner service instance.
#[derive(Debug)]
pub struct CircuitBreakerService<S, P, I, T> {
    shared: Arc<Shared<P, I, T>>,
    inner: S,
    state: State,
}

impl<S, P, I, T> CircuitBreakerService<S, P, I, T>
where
    S: Service,
    P: FailurePolicy,
    T: FailurePredicate<S::Error>,
    I: Instrument,
{
    pub fn new(inner: S, failure_policy: P, failure_predicate: T, instrument: I) -> Self {
        Self {
            shared: Arc::new(Shared {
                state_machine: StateMachine::new(failure_policy, instrument),
                failure_predicate,
            }),
            state: State::Closed,
            inner,
        }
    }
}

impl<S, P, I, T> Service for CircuitBreakerService<S, P, I, T>
where
    S: Service,
    P: FailurePolicy,
    T: FailurePredicate<S::Error>,
    I: Instrument,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = Error<S::Error>;
    type Future = ResponseFuture<S::Future, P, I, T>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if let State::Closed = self.state {
            if let Some(until) = self.shared.state_machine.is_open() {
                self.state = State::Open(Delay::new(until))
            }
        }

        if let State::Open(ref mut delay) = self.state {
            let _ = try_ready!(delay.poll().map_err(|_| Error::Rejected));
        }

        self.state = State::Closed;
        self.inner.poll_ready().map_err(Error::Upstream)
    }

    #[inline]
    fn call(&mut self, req: Self::Request) -> Self::Future {
        ResponseFuture {
            future: self.inner.call(req),
            shared: self.shared.clone(),
            ask: false,
        }
    }
}

pub struct ResponseFuture<F, P, I, T> {
    future: F,
    shared: Arc<Shared<P, I, T>>,
    ask: bool,
}

impl<F, P, I, T> Future for ResponseFuture<F, P, I, T>
where
    F: Future,
    P: FailurePolicy,
    T: FailurePredicate<F::Error>,
    I: Instrument,
{
    type Item = F::Item;
    type Error = Error<F::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if !self.ask {
            self.ask = true;
            if !self.shared.state_machine.is_call_permitted() {
                return Err(Error::Rejected);
            }
        }

        match self.future.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(rep)) => {
                self.shared.state_machine.on_success();
                Ok(Async::Ready(rep))
            }
            Err(err) => {
                if self.shared.failure_predicate.is_err(&err) {
                    self.shared.state_machine.on_error();
                } else {
                    self.shared.state_machine.on_success();
                }

                Err(Error::Upstream(err))
            }
        }
    }
}
