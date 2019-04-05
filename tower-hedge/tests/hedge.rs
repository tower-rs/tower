extern crate futures;
extern crate tokio_executor;
extern crate tokio_mock_task;
extern crate tokio_timer;
extern crate tower_hedge as hedge;
extern crate tower_mock;
extern crate tower_service;

#[macro_use]
mod support;
use support::*;

use futures::Future;
use hedge::Policy;
use tower_service::Service;

use std::time::Duration;

#[test]
fn hedge_orig_completes_first() {
    let (mut service, mut handle) = new_service(TestPolicy);

    mocked(|timer, _| {
        let mut fut = service.call("orig");
        // Check that orig request has been issued.
        let req = handle.next_request().expect("orig");
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());

        // Check hedge has not been issued.
        assert!(handle.poll_request().unwrap().is_not_ready());
        advance(timer, ms(10));
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());
        // Check that the hedge has been issued.
        let _hedge_req = handle.next_request().expect("hedge");

        req.respond("orig-done");
        // Check that fut gets orig response.
        assert_eq!(fut.wait().unwrap(), "orig-done");
    });
}

#[test]
fn hedge_hedge_completes_first() {
    let (mut service, mut handle) = new_service(TestPolicy);

    mocked(|timer, _| {
        let mut fut = service.call("orig");
        // Check that orig request has been issued.
        let _req = handle.next_request().expect("orig");
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());

        // Check hedge has not been issued.
        assert!(handle.poll_request().unwrap().is_not_ready());
        advance(timer, ms(10));
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());

        // Check that the hedge has been issued.
        let hedge_req = handle.next_request().expect("hedge");
        hedge_req.respond("hedge-done");
        // Check that fut gets hedge response.
        assert_eq!(fut.wait().unwrap(), "hedge-done");
    });
}

#[test]
fn completes_before_hedge() {
    let (mut service, mut handle) = new_service(TestPolicy);

    mocked(|_, _| {
        let mut fut = service.call("orig");
        // Check that orig request has been issued.
        let req = handle.next_request().expect("orig");
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());

        req.respond("orig-done");
        // Check hedge has not been issued.
        assert!(handle.poll_request().unwrap().is_not_ready());
        // Check that fut gets orig response.
        assert_eq!(fut.wait().unwrap(), "orig-done");
    });
}

#[test]
fn request_not_retyable() {
    let (mut service, mut handle) = new_service(TestPolicy);

    mocked(|timer, _| {
        let mut fut = service.call(NOT_RETRYABLE);
        // Check that orig request has been issued.
        let req = handle.next_request().expect("orig");
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());

        // Check hedge has not been issued.
        assert!(handle.poll_request().unwrap().is_not_ready());
        advance(timer, ms(10));
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());
        // Check hedge has not been issued.
        assert!(handle.poll_request().unwrap().is_not_ready());

        req.respond("orig-done");
        // Check that fut gets orig response.
        assert_eq!(fut.wait().unwrap(), "orig-done");
    });
}

#[test]
fn request_not_clonable() {
    let (mut service, mut handle) = new_service(TestPolicy);

    mocked(|timer, _| {
        let mut fut = service.call(NOT_CLONABLE);
        // Check that orig request has been issued.
        let req = handle.next_request().expect("orig");
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());

        // Check hedge has not been issued.
        assert!(handle.poll_request().unwrap().is_not_ready());
        advance(timer, ms(10));
        // Check fut is not ready.
        assert!(fut.poll().unwrap().is_not_ready());
        // Check hedge has not been issued.
        assert!(handle.poll_request().unwrap().is_not_ready());

        req.respond("orig-done");
        // Check that fut gets orig response.
        assert_eq!(fut.wait().unwrap(), "orig-done");
    });
}

type Req = &'static str;
type Res = &'static str;
type Mock = tower_mock::Mock<Req, Res>;
type Handle = tower_mock::Handle<Req, Res>;

static NOT_RETRYABLE: &'static str = "NOT_RETRYABLE";
static NOT_CLONABLE: &'static str = "NOT_CLONABLE";

#[derive(Clone)]
struct TestPolicy;

impl hedge::Policy<Req> for TestPolicy {
    fn can_retry(&self, req: &Req) -> bool {
        *req != NOT_RETRYABLE
    }

    fn clone_request(&self, req: &Req) -> Option<Req> {
        if *req == NOT_CLONABLE {
            None
        } else {
            Some(req)
        }
    }
}

fn new_service<P: Policy<Req> + Clone>(policy: P) -> (hedge::Service<Mock, P>, Handle) {
    let (service, handle) = Mock::new();
    
    let mock_latencies: [u64; 10] = [1, 1, 1, 1, 1, 1, 1, 1, 10, 10];

    let service = hedge::service_with_mock_latencies(
        service,
        policy,
        10,
        0.9,
        Duration::from_secs(60),
        &mock_latencies
    );
    (service, handle)
}
