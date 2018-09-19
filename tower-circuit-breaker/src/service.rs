use std::sync::Arc;

use futures::{Async, Future, Poll};
use tower_service::Service;

use super::classification::{Callback, Classification, ErrorClassifier, ResponseClassifier};
use super::error::Error;
use super::failure_policy::FailurePolicy;
use super::instrument::Instrument;
use super::state_machine::StateMachine;

#[derive(Debug)]
struct Classifier<E, R> {
    error: E,
    response: R,
}

/// Circuit breaker services, it wraps an inner service instance.
#[derive(Debug)]
pub struct CircuitBreaker<S, P, I, E, R> {
    inner: S,
    state_machine: Arc<StateMachine<P, I>>,
    classifier: Arc<Classifier<E, R>>,
}

/// Record response status.
#[derive(Debug)]
pub struct CallbackHandler<P, I>(Arc<StateMachine<P, I>>);

impl<S, P, I, E, R> CircuitBreaker<S, P, I, E, R>
where
    S: Service,
    P: FailurePolicy,
    I: Instrument,
    E: ErrorClassifier<S::Error>,
    R: ResponseClassifier<S::Response, CallbackHandler<P, I>>,
{
    pub fn new(
        inner: S,
        failure_policy: P,
        instrument: I,
        error_classifier: E,
        response_classifier: R,
    ) -> Self {
        Self {
            inner,
            state_machine: Arc::new(StateMachine::new(failure_policy, instrument)),
            classifier: Arc::new(Classifier {
                error: error_classifier,
                response: response_classifier,
            }),
        }
    }
}

impl<P, I> Callback for CallbackHandler<P, I>
where
    P: FailurePolicy,
    I: Instrument,
{
    fn notify(self, classification: Classification) {
        match classification {
            Classification::Success => self.0.on_success(),
            Classification::Failure => self.0.on_error(),
        }
    }
}

impl<S, P, I, E, R> Service for CircuitBreaker<S, P, I, E, R>
where
    S: Service,
    P: FailurePolicy,
    I: Instrument,
    E: ErrorClassifier<S::Error>,
    R: ResponseClassifier<S::Response, CallbackHandler<P, I>>,
{
    type Request = S::Request;
    type Response = R::Response;
    type Error = Error<S::Error>;
    type Future = ResponseFuture<S::Future, P, I, E, R>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        try_ready!(self.state_machine.poll_ready().map_err(|_| Error::Rejected));
        self.inner.poll_ready().map_err(Error::Upstream)
    }

    #[inline]
    fn call(&mut self, req: Self::Request) -> Self::Future {
        ResponseFuture {
            future: self.inner.call(req),
            state_machine: self.state_machine.clone(),
            classifier: self.classifier.clone(),
            ask: false,
        }
    }
}

pub struct ResponseFuture<F, P, I, E, R> {
    future: F,
    state_machine: Arc<StateMachine<P, I>>,
    classifier: Arc<Classifier<E, R>>,
    ask: bool,
}

impl<F, P, I, E, R> Future for ResponseFuture<F, P, I, E, R>
where
    F: Future,
    P: FailurePolicy,
    I: Instrument,
    E: ErrorClassifier<F::Error>,
    R: ResponseClassifier<F::Item, CallbackHandler<P, I>>,
{
    type Item = R::Response;
    type Error = Error<F::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if !self.ask {
            self.ask = true;
            if !self.state_machine.is_call_permitted() {
                return Err(Error::Rejected);
            }
        }

        let resp = match self.future.poll() {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(resp)) => resp,
            Err(err) => {
                match self.classifier.error.classify(&err) {
                    Classification::Success => self.state_machine.on_success(),
                    Classification::Failure => self.state_machine.on_error(),
                }
                return Err(Error::Upstream(err));
            }
        };

        let callback = CallbackHandler(self.state_machine.clone());
        let resp = self.classifier.response.classify(resp, callback);

        Ok(Async::Ready(resp))
    }
}

#[cfg(test)]
mod tests {
    use tokio_test::MockTask;
    use tower_mock;

    use super::*;
    use backoff;
    use classification::AlwaysSuccess;
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
        CircuitBreaker<
            Mock,
            failure_policy::ConsecutiveFailures<backoff::Constant>,
            (),
            IsErr,
            AlwaysSuccess,
        >,
        Handle,
    ) {
        let (service, handle) = Mock::new();
        let backoff = backoff::constant(3.seconds());
        let policy = failure_policy::consecutive_failures(1, backoff);
        let service = CircuitBreaker::new(service, policy, (), IsErr, AlwaysSuccess);
        (service, handle)
    }

    struct IsErr;

    impl ErrorClassifier<tower_mock::Error<bool>> for IsErr {
        fn classify(&self, err: &tower_mock::Error<bool>) -> Classification {
            match err {
                tower_mock::Error::Other(ref err) if !err => Classification::Success,
                _ => Classification::Failure,
            }
        }
    }
}
