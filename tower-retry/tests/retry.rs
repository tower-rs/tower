extern crate futures;
extern crate tower_mock;
extern crate tower_retry;
extern crate tower_service;

use futures::{future, Future};
use tower_retry::Policy;
use tower_service::Service;

#[test]
fn retry_errors() {
    let (mut service, mut handle) = new_service(RetryErrors);

    let mut fut = service.call("hello");

    let req1 = handle.next_request().unwrap();
    assert_eq!(*req1, "hello");
    req1.error("retry me");

    assert_not_ready(&mut fut);

    let req2 = handle.next_request().unwrap();
    assert_eq!(*req2, "hello");
    req2.respond("world");

    assert_eq!(fut.wait().unwrap(), "world");
}

#[test]
fn retry_limit() {
    let (mut service, mut handle) = new_service(Limit(2));

    let mut fut = service.call("hello");

    let req1 = handle.next_request().unwrap();
    assert_eq!(*req1, "hello");
    req1.error("retry 1");

    assert_not_ready(&mut fut);

    let req2 = handle.next_request().unwrap();
    assert_eq!(*req2, "hello");
    req2.error("retry 2");

    assert_not_ready(&mut fut);

    let req3 = handle.next_request().unwrap();
    assert_eq!(*req3, "hello");
    req3.error("retry 3");

    assert_eq!(fut.wait().unwrap_err(), tower_mock::Error::Other("retry 3"));
}

#[test]
fn retry_error_inspection() {
    let (mut service, mut handle) = new_service(UnlessErr("reject"));

    let mut fut = service.call("hello");

    let req1 = handle.next_request().unwrap();
    assert_eq!(*req1, "hello");
    req1.error("retry 1");

    assert_not_ready(&mut fut);

    let req2 = handle.next_request().unwrap();
    assert_eq!(*req2, "hello");
    req2.error("reject");
    assert_eq!(fut.wait().unwrap_err(), tower_mock::Error::Other("reject"));
}

#[test]
fn retry_cannot_clone_request() {
    let (mut service, mut handle) = new_service(CannotClone);

    let fut = service.call("hello");

    let req1 = handle.next_request().unwrap();
    assert_eq!(*req1, "hello");
    req1.error("retry 1");

    assert_eq!(fut.wait().unwrap_err(), tower_mock::Error::Other("retry 1"));
}

#[test]
fn success_with_cannot_clone() {
    // Even though the request couldn't be cloned, if the first request succeeds,
    // it should succeed overall.
    let (mut service, mut handle) = new_service(CannotClone);

    let fut = service.call("hello");

    let req1 = handle.next_request().unwrap();
    assert_eq!(*req1, "hello");
    req1.respond("world");

    assert_eq!(fut.wait().unwrap(), "world");
}

type Req = &'static str;
type Res = &'static str;
type InnerError = &'static str;
type Error = tower_mock::Error<InnerError>;
type Mock = tower_mock::Mock<Req, Res, InnerError>;
type Handle = tower_mock::Handle<Req, Res, InnerError>;

#[derive(Clone)]
struct RetryErrors;

impl Policy<Req, Res, Error> for RetryErrors {
    type Future = future::FutureResult<Self, ()>;
    fn retry(&self, _: &Req, result: Result<&Res, &Error>) -> Option<Self::Future> {
        if result.is_err() {
            Some(future::ok(RetryErrors))
        } else {
            None
        }
    }

    fn clone_request(&self, req: &Req) -> Option<Req> {
        Some(*req)
    }
}

#[derive(Clone)]
struct Limit(usize);

impl Policy<Req, Res, Error> for Limit {
    type Future = future::FutureResult<Self, ()>;
    fn retry(&self, _: &Req, result: Result<&Res, &Error>) -> Option<Self::Future> {
        if result.is_err() && self.0 > 0 {
            Some(future::ok(Limit(self.0 - 1)))
        } else {
            None
        }
    }

    fn clone_request(&self, req: &Req) -> Option<Req> {
        Some(*req)
    }
}

#[derive(Clone)]
struct UnlessErr(InnerError);

impl Policy<Req, Res, Error> for UnlessErr {
    type Future = future::FutureResult<Self, ()>;
    fn retry(&self, _: &Req, result: Result<&Res, &Error>) -> Option<Self::Future> {
        result
            .err()
            .and_then(|err| {
                if err != &tower_mock::Error::Other(self.0) {
                    Some(future::ok(self.clone()))
                } else {
                    None
                }
            })

    }

    fn clone_request(&self, req: &Req) -> Option<Req> {
        Some(*req)
    }
}

#[derive(Clone)]
struct CannotClone;

impl Policy<Req, Res, Error> for CannotClone {
    type Future = future::FutureResult<Self, ()>;
    fn retry(&self, _: &Req, _: Result<&Res, &Error>) -> Option<Self::Future> {
        unreachable!("retry cannot be called since request isn't cloned");
    }

    fn clone_request(&self, _req: &Req) -> Option<Req> {
        None
    }
}

fn new_service<P: Policy<Req, Res, Error> + Clone>(policy: P) -> (tower_retry::Retry<P, Mock>, Handle) {
    let (service, handle) = Mock::new();
    let service = tower_retry::Retry::new(policy, service);
    (service, handle)
}

fn assert_not_ready<F: Future>(f: &mut F) where F::Error: ::std::fmt::Debug {
    use futures::future;
    future::poll_fn(|| {
        assert!(f.poll().unwrap().is_not_ready());
        Ok::<_, ()>(().into())
    }).wait().unwrap();
}
