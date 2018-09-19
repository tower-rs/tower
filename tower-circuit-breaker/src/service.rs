use std::error::Error as StdError;
use std::fmt::{self, Display};
use std::sync::Arc;

use futures::{Async, Future, Poll};
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
struct Shared<P, I, T> {
    state_machine: StateMachine<P, I>,
    failure_predicate: T,
}

/// Circuit breaker services, it wraps an inner service instance.
#[derive(Debug)]
pub struct CircuitBreaker<S, P, I, T> {
    shared: Arc<Shared<P, I, T>>,
    inner: S,
}

impl<S, P, I, T> CircuitBreaker<S, P, I, T>
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
            inner,
        }
    }
}

impl<S, P, I, T> Service for CircuitBreaker<S, P, I, T>
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
        let _ready = try_ready!(
            self.shared
                .state_machine
                .poll_ready()
                .map_err(|_| Error::Rejected)
        );
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tower_mock;

    use super::*;
    use failure_policy;
    use backoff;
    use mock_clock as clock;

    #[test]
    fn basic_error_handle() {
        clock::freeze(|_time| {
            let (mut service, mut handle) = new_service();

            // ok
            assert_eq!(Async::Ready(()), service.poll_ready().unwrap());

            let mut r1 = service.call("req 1");
            let req = handle.next_request().unwrap();

            req.respond("res 1");
            match r1.poll() {
                Ok(Async::Ready(s)) if s == "res 1" => {},
                x => unreachable!("{:?}", x)
            }

            // err not matched
            assert_eq!(Async::Ready(()), service.poll_ready().unwrap());
            let mut r2 = service.call("req 2");
            let req = handle.next_request().unwrap();

            req.error(false);
            match r2.poll() {
                Err(Error::Upstream(tower_mock::Error::Other(ok))) if !ok => {}
                x => unreachable!("{:?}", x),
            }

            // err matched
            assert_eq!(Async::Ready(()), service.poll_ready().unwrap());
            let mut r3 = service.call("req 2");
            let req = handle.next_request().unwrap();

            req.error(true);
            match r3.poll() {
                Err(Error::Upstream(tower_mock::Error::Other(ok))) if ok => {}
                x => unreachable!("{:?}", x),
            }

            match service.poll_ready() {
                Ok(Async::NotReady) => {}
                x => unreachable!("{:?}", x),
            }
        })
    }

    type Mock = tower_mock::Mock<&'static str, &'static str, bool>;
    type Handle = tower_mock::Handle<&'static str, &'static str, bool>;

    fn new_service() -> (
        CircuitBreaker<Mock, failure_policy::ConsecutiveFailures<backoff::Constant>, (), IsErr>,
        Handle,
    ) {
        let (service, handle) = Mock::new();
        let backoff = backoff::constant(Duration::from_secs(3));
        let policy = failure_policy::consecutive_failures(1, backoff);
        let service = CircuitBreaker::new(service, policy, IsErr, ());
        (service, handle)
    }

    struct IsErr;

    impl FailurePredicate<tower_mock::Error<bool>> for IsErr {
        fn is_err(&self, err: &tower_mock::Error<bool>) -> bool {
            match err {
                tower_mock::Error::Other(ref err) => *err,
                _ => true,
            }
        }
    }

}