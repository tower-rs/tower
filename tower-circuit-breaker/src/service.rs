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
    use tokio_test::MockTask;
    use tower_mock;

    use super::*;
    use backoff;
    use failure_policy;
    use mock_clock::{self as clock, IntoDuration};

    macro_rules! assert_ready {
        ($expected:expr, $f:expr) => {{
            match $f.unwrap() {
                Async::Ready(v) => assert_eq!($expected, v),
                Async::NotReady => panic!("NotReady"),
            }
        }};
    }

    macro_rules! assert_err {
        ($expected:expr, $f:expr) => {{
            match $f {
                Err(Error::Upstream(tower_mock::Error::Other(err))) => assert_eq!($expected, err),
                x => panic!("{:?}", x),
            }
        }};
    }

    macro_rules! assert_not_ready {
        ($f:expr) => {{
            match $f.unwrap() {
                Async::NotReady => {}
                x => panic!("{:?}", x),
            }
        }};
    }

    #[test]
    fn from_closed_to_open_and_back_to_closed() {
        clock::freeze(|time| {
            let (mut service, mut handle) = new_service();

            assert_ready!((), service.poll_ready());

            // perform a successful request
            let mut r1 = service.call("req 1");
            let req = handle.next_request().unwrap();
            req.respond("res 1");

            assert_ready!("res 1", r1.poll());
            assert_ready!((), service.poll_ready());

            // perform a request error, which doesn't translate to a circuit breaker's error.
            let mut r2 = service.call("req 2");
            let req = handle.next_request().unwrap();
            req.error(false);

            assert_err!(false, r2.poll());
            assert_ready!((), service.poll_ready());

            // perform a request error which translates to a circuit breaker's error.
            let mut r3 = service.call("req 2");
            let req = handle.next_request().unwrap();

            req.error(true);
            assert_err!(true, r3.poll());
            assert_not_ready!(service.poll_ready());

            // after 3s the circuit breaker is becoming closed.
            time.advance(3.seconds());
            assert_ready!((), service.poll_ready());
        })
    }

    #[test]
    fn wait_until_service_becomes_ready() {
        clock::freeze(|time| {
            let (mut service, mut handle) = new_service();
            let mut task1 = MockTask::new();
            let mut task2 = MockTask::new();

            assert_eq!(Async::Ready(()), service.poll_ready().unwrap());

            let mut r1 = service.call("req 2");
            let req = handle.next_request().unwrap();

            req.error(true);
            assert_err!(true, r1.poll());

            task1.enter(|| {
                assert_not_ready!(service.poll_ready());
            });

            task2.enter(|| {
                assert_not_ready!(service.poll_ready());
            });

            assert_eq!(false, task1.is_notified());
            assert_eq!(false, task2.is_notified());

            // notify the last task after 3s.
            time.advance(3.seconds());

            assert_eq!(false, task1.is_notified());
            assert_eq!(true, task2.is_notified());
        });
    }

    type Mock = tower_mock::Mock<&'static str, &'static str, bool>;
    type Handle = tower_mock::Handle<&'static str, &'static str, bool>;

    fn new_service() -> (
        CircuitBreaker<Mock, failure_policy::ConsecutiveFailures<backoff::Constant>, (), IsErr>,
        Handle,
    ) {
        let (service, handle) = Mock::new();
        let backoff = backoff::constant(3.seconds());
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
