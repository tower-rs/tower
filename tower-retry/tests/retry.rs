use futures_util::{future, pin_mut};
use std::future::Future;
use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok, task};
use tower_retry::Policy;
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn retry_errors() {
    task::mock(|cx| {
        let (mut service, handle) = new_service(RetryErrors);
        pin_mut!(handle);

        assert_ready_ok!(service.poll_ready(cx));

        let fut = service.call("hello");
        pin_mut!(fut);

        assert_request_eq!(handle.as_mut(), "hello").send_error("retry me");

        assert_pending!(fut.as_mut().poll(cx));

        assert_request_eq!(handle.as_mut(), "hello").send_response("world");

        assert_ready_ok!(fut.poll(cx), "world");
    });
}

#[test]
fn retry_limit() {
    task::mock(|cx| {
        let (mut service, handle) = new_service(Limit(2));
        pin_mut!(handle);

        assert_ready_ok!(service.poll_ready(cx));

        let fut = service.call("hello");
        pin_mut!(fut);

        assert_request_eq!(handle.as_mut(), "hello").send_error("retry 1");
        assert_pending!(fut.as_mut().poll(cx));

        assert_request_eq!(handle.as_mut(), "hello").send_error("retry 2");
        assert_pending!(fut.as_mut().poll(cx));

        assert_request_eq!(handle.as_mut(), "hello").send_error("retry 3");
        assert_eq!(assert_ready_err!(fut.poll(cx)).to_string(), "retry 3");
    });
}

#[test]
fn retry_error_inspection() {
    task::mock(|cx| {
        let (mut service, handle) = new_service(UnlessErr("reject"));
        pin_mut!(handle);

        assert_ready_ok!(service.poll_ready(cx));
        let fut = service.call("hello");
        pin_mut!(fut);

        assert_request_eq!(handle.as_mut(), "hello").send_error("retry 1");
        assert_pending!(fut.as_mut().poll(cx));

        assert_request_eq!(handle.as_mut(), "hello").send_error("reject");
        assert_eq!(assert_ready_err!(fut.poll(cx)).to_string(), "reject");
    });
}

#[test]
fn retry_cannot_clone_request() {
    task::mock(|cx| {
        let (mut service, handle) = new_service(CannotClone);
        pin_mut!(handle);

        assert_ready_ok!(service.poll_ready(cx));
        let fut = service.call("hello");
        pin_mut!(fut);

        assert_request_eq!(handle, "hello").send_error("retry 1");
        assert_eq!(assert_ready_err!(fut.poll(cx)).to_string(), "retry 1");
    });
}

#[test]
fn success_with_cannot_clone() {
    task::mock(|cx| {
        // Even though the request couldn't be cloned, if the first request succeeds,
        // it should succeed overall.
        let (mut service, handle) = new_service(CannotClone);
        pin_mut!(handle);

        assert_ready_ok!(service.poll_ready(cx));
        let fut = service.call("hello");
        pin_mut!(fut);

        assert_request_eq!(handle, "hello").send_response("world");
        assert_ready_ok!(fut.poll(cx), "world");
    });
}

type Req = &'static str;
type Res = &'static str;
type InnerError = &'static str;
type Error = Box<dyn std::error::Error + Send + Sync>;
type Mock = mock::Mock<Req, Res>;
type Handle = mock::Handle<Req, Res>;

#[derive(Clone)]
struct RetryErrors;

impl Policy<Req, Res, Error> for RetryErrors {
    type Future = future::Ready<Self>;
    fn retry(&self, _: &Req, result: Result<&Res, &Error>) -> Option<Self::Future> {
        if result.is_err() {
            Some(future::ready(RetryErrors))
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
    type Future = future::Ready<Self>;
    fn retry(&self, _: &Req, result: Result<&Res, &Error>) -> Option<Self::Future> {
        if result.is_err() && self.0 > 0 {
            Some(future::ready(Limit(self.0 - 1)))
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
    type Future = future::Ready<Self>;
    fn retry(&self, _: &Req, result: Result<&Res, &Error>) -> Option<Self::Future> {
        result.err().and_then(|err| {
            if err.to_string() != self.0 {
                Some(future::ready(self.clone()))
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
    type Future = future::Ready<Self>;
    fn retry(&self, _: &Req, _: Result<&Res, &Error>) -> Option<Self::Future> {
        unreachable!("retry cannot be called since request isn't cloned");
    }

    fn clone_request(&self, _req: &Req) -> Option<Req> {
        None
    }
}

fn new_service<P: Policy<Req, Res, Error> + Clone>(
    policy: P,
) -> (tower_retry::Retry<P, Mock>, Handle) {
    let (service, handle) = mock::pair();
    let service = tower_retry::Retry::new(policy, service);
    (service, handle)
}
