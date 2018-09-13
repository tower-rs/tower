extern crate futures;
extern crate tokio_test;
extern crate tower_circuit_breaker;
extern crate tower_mock;
extern crate tower_service;

use std::time::Duration;

use futures::{Async, Future};
use tokio_test::MockTask;
use tower_service::Service;

use tower_circuit_breaker::{
    backoff, failure_policy, CircuitBreakerService, Error, FailurePredicate,
};

#[test]
fn basic_error_handle() {
    let (mut service, mut handle) = new_service();

    // ok
    assert_eq!(Async::Ready(()), service.poll_ready().unwrap());
    let r1 = service.call("req 1");
    let req = handle.next_request().unwrap();

    req.respond("res 1");
    assert_eq!("res 1", r1.wait().unwrap());

    // err not matched
    assert_eq!(Async::Ready(()), service.poll_ready().unwrap());
    let r2 = service.call("req 2");
    let req = handle.next_request().unwrap();

    req.error(false);
    assert_eq!(
        Err(Error::Inner(tower_mock::Error::Other(false))),
        r2.wait()
    );

    // err matched
    assert_eq!(Async::Ready(()), service.poll_ready().unwrap());
    let r3 = service.call("req 2");
    let req = handle.next_request().unwrap();

    req.error(true);
    assert_eq!(Err(Error::Inner(tower_mock::Error::Other(true))), r3.wait());

    assert_eq!(Err(Error::Rejected), service.poll_ready());
}

type Mock = tower_mock::Mock<&'static str, &'static str, bool>;
type Handle = tower_mock::Handle<&'static str, &'static str, bool>;

fn new_service() -> (
    CircuitBreakerService<Mock, failure_policy::ConsecutiveFailures<backoff::Constant>, (), IsErr>,
    Handle,
) {
    let (service, handle) = Mock::new();
    let backoff = backoff::constant(Duration::from_secs(3));
    let policy = failure_policy::consecutive_failures(1, backoff);
    let service = CircuitBreakerService::new(service, policy, IsErr, ());
    (service, handle)
}

struct IsErr;

impl FailurePredicate<tower_mock::Error<bool>> for IsErr {
    fn is_err(&self, err: &tower_mock::Error<bool>) -> bool {
        match err {
            ::tower_mock::Error::Other(ref err) => *err,
            _ => true,
        }
    }
}
