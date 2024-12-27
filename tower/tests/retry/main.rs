#![cfg(feature = "retry")]
#[path = "../support.rs"]
mod support;

use futures_util::future;
use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok, task};
use tower::retry::{Outcome, Policy};
use tower_test::{assert_request_eq, mock};

#[tokio::test(flavor = "current_thread")]
async fn retry_errors() {
    let _t = support::trace_init();

    let (mut service, mut handle) = new_service(RetryErrors);

    assert_ready_ok!(service.poll_ready());

    let mut fut = task::spawn(service.call("hello"));

    assert_request_eq!(handle, "hello").send_error("retry me");

    assert_pending!(fut.poll());

    assert_request_eq!(handle, "hello").send_response("world");

    assert_eq!(fut.into_inner().await.unwrap(), "world");
}

#[tokio::test(flavor = "current_thread")]
async fn retry_limit() {
    let _t = support::trace_init();

    let (mut service, mut handle) = new_service(Limit(2));

    assert_ready_ok!(service.poll_ready());

    let mut fut = task::spawn(service.call("hello"));

    assert_request_eq!(handle, "hello").send_error("retry 1");
    assert_pending!(fut.poll());

    assert_request_eq!(handle, "hello").send_error("retry 2");
    assert_pending!(fut.poll());

    assert_request_eq!(handle, "hello").send_error("retry 3");
    assert_eq!(assert_ready_err!(fut.poll()).to_string(), "retry 3");
}

#[tokio::test(flavor = "current_thread")]
async fn retry_error_inspection() {
    let _t = support::trace_init();

    let (mut service, mut handle) = new_service(UnlessErr("reject"));

    assert_ready_ok!(service.poll_ready());
    let mut fut = task::spawn(service.call("hello"));

    assert_request_eq!(handle, "hello").send_error("retry 1");
    assert_pending!(fut.poll());

    assert_request_eq!(handle, "hello").send_error("reject");
    assert_eq!(assert_ready_err!(fut.poll()).to_string(), "reject");
}

#[tokio::test(flavor = "current_thread")]
async fn retry_mutating_policy() {
    let _t = support::trace_init();

    let (mut service, mut handle) = new_service(MutatingPolicy { remaining: 2 });

    assert_ready_ok!(service.poll_ready());
    let mut fut = task::spawn(service.call("hello"));

    assert_request_eq!(handle, "hello").send_response("world");
    assert_pending!(fut.poll());
    // the policy alters the request. in real life, this might be setting a header
    assert_request_eq!(handle, "retrying").send_response("world");

    assert_pending!(fut.poll());

    assert_request_eq!(handle, "retrying").send_response("world");

    assert_ready_err!(fut.poll(), "out of retries");
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
    type Future = future::Ready<()>;

    type CloneableRequest = Req;

    fn retry(
        &mut self,
        _req: &mut Self::CloneableRequest,
        result: Result<Res, Error>,
    ) -> Outcome<Self::Future, Res, Error> {
        if result.is_err() {
            Outcome::Retry(future::ready(()))
        } else {
            Outcome::Return(result)
        }
    }

    fn create_cloneable_request(&mut self, req: Req) -> Self::CloneableRequest {
        req
    }

    fn clone_request(&mut self, req: &Self::CloneableRequest) -> Req {
        *req
    }
}

#[derive(Clone)]
struct Limit(usize);

impl Policy<Req, Res, Error> for Limit {
    type Future = future::Ready<()>;

    type CloneableRequest = Req;

    fn retry(
        &mut self,
        _req: &mut Self::CloneableRequest,
        result: Result<Res, Error>,
    ) -> Outcome<Self::Future, Res, Error> {
        if result.is_err() && self.0 > 0 {
            self.0 -= 1;
            Outcome::Retry(future::ready(()))
        } else {
            Outcome::Return(result)
        }
    }

    fn create_cloneable_request(&mut self, req: Req) -> Self::CloneableRequest {
        req
    }

    fn clone_request(&mut self, req: &Self::CloneableRequest) -> Req {
        *req
    }
}

#[derive(Clone)]
struct UnlessErr(InnerError);

impl Policy<Req, Res, Error> for UnlessErr {
    type Future = future::Ready<()>;

    type CloneableRequest = Req;

    fn retry(
        &mut self,
        _req: &mut Self::CloneableRequest,
        result: Result<Res, Error>,
    ) -> Outcome<Self::Future, Res, Error> {
        if result
            .as_ref()
            .err()
            .map(|err| err.to_string() != self.0)
            .unwrap_or_default()
        {
            Outcome::Retry(future::ready(()))
        } else {
            Outcome::Return(result)
        }
    }

    fn create_cloneable_request(&mut self, req: Req) -> Self::CloneableRequest {
        req
    }

    fn clone_request(&mut self, req: &Self::CloneableRequest) -> Req {
        *req
    }
}

/// Test policy that changes the request to `retrying` during retries and the result to `"out of retries"`
/// when retries are exhausted.
#[derive(Clone)]
struct MutatingPolicy {
    remaining: usize,
}

impl Policy<Req, Res, Error> for MutatingPolicy
where
    Error: From<&'static str>,
{
    type Future = future::Ready<()>;

    type CloneableRequest = Req;

    fn retry(
        &mut self,
        req: &mut Self::CloneableRequest,
        _result: Result<Res, Error>,
    ) -> Outcome<Self::Future, Res, Error> {
        if self.remaining == 0 {
            Outcome::Return(Err("out of retries".into()))
        } else {
            *req = "retrying";
            self.remaining -= 1;
            Outcome::Retry(future::ready(()))
        }
    }

    fn create_cloneable_request(&mut self, req: Req) -> Self::CloneableRequest {
        req
    }

    fn clone_request(&mut self, req: &Self::CloneableRequest) -> Req {
        *req
    }
}

fn new_service<P: Policy<Req, Res, Error> + Clone>(
    policy: P,
) -> (mock::Spawn<tower::retry::Retry<P, Mock>>, Handle) {
    let retry = tower::retry::RetryLayer::new(policy);
    mock::spawn_layer(retry)
}
