use futures_util::pin_mut;
use std::future::Future;
use std::time::Duration;
use tokio_test::{assert_pending, assert_ready, assert_ready_ok, clock, task};
use tower_hedge::{Hedge, Policy};
use tower_service::Service;
use tower_test::assert_request_eq;

#[test]
fn hedge_orig_completes_first() {
    task::mock(|cx| {
        clock::mock(|time| {
            let (mut service, handle) = new_service(TestPolicy);
            pin_mut!(handle);

            assert_ready_ok!(service.poll_ready(cx));
            let fut = service.call("orig");
            pin_mut!(fut);

            // Check that orig request has been issued.
            let req = assert_request_eq!(handle, "orig");
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));

            // Check hedge has not been issued.
            assert_pending!(handle.as_mut().poll_request(cx));
            time.advance(Duration::from_millis(10));
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));
            // Check that the hedge has been issued.
            let _hedge_req = assert_request_eq!(handle, "orig");

            req.send_response("orig-done");
            // Check that fut gets orig response.
            assert_eq!(assert_ready_ok!(fut.as_mut().poll(cx)), "orig-done");
        });
    });
}

#[test]
fn hedge_hedge_completes_first() {
    task::mock(|cx| {
        clock::mock(|time| {
            let (mut service, handle) = new_service(TestPolicy);
            pin_mut!(handle);

            assert_ready_ok!(service.poll_ready(cx));
            let fut = service.call("orig");
            pin_mut!(fut);
            // Check that orig request has been issued.
            let _req = assert_request_eq!(handle, "orig");
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));

            // Check hedge has not been issued.
            assert_pending!(handle.as_mut().poll_request(cx));
            time.advance(Duration::from_millis(10));
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));

            // Check that the hedge has been issued.
            let hedge_req = assert_request_eq!(handle, "orig");
            hedge_req.send_response("hedge-done");
            // Check that fut gets hedge response.
            assert_eq!(assert_ready_ok!(fut.as_mut().poll(cx)), "hedge-done");
        });
    });
}

#[test]
fn completes_before_hedge() {
    task::mock(|cx| {
        clock::mock(|_| {
            let (mut service, handle) = new_service(TestPolicy);
            pin_mut!(handle);

            assert_ready_ok!(service.poll_ready(cx));
            let fut = service.call("orig");
            pin_mut!(fut);
            // Check that orig request has been issued.
            let req = assert_request_eq!(handle, "orig");
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));

            req.send_response("orig-done");
            // Check hedge has not been issued.
            assert_pending!(handle.as_mut().poll_request(cx));
            // Check that fut gets orig response.
            assert_eq!(assert_ready_ok!(fut.as_mut().poll(cx)), "orig-done");
        });
    });
}

#[test]
fn request_not_retyable() {
    task::mock(|cx| {
        clock::mock(|time| {
            let (mut service, handle) = new_service(TestPolicy);
            pin_mut!(handle);

            assert_ready_ok!(service.poll_ready(cx));
            let fut = service.call(NOT_RETRYABLE);
            pin_mut!(fut);
            // Check that orig request has been issued.
            let req = assert_request_eq!(handle, NOT_RETRYABLE);
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));

            // Check hedge has not been issued.
            assert_pending!(handle.as_mut().poll_request(cx));
            time.advance(Duration::from_millis(10));
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));
            // Check hedge has not been issued.
            assert_pending!(handle.as_mut().poll_request(cx));

            req.send_response("orig-done");
            // Check that fut gets orig response.
            assert_eq!(assert_ready_ok!(fut.as_mut().poll(cx)), "orig-done");
        });
    });
}

#[test]
fn request_not_clonable() {
    task::mock(|cx| {
        clock::mock(|time| {
            let (mut service, handle) = new_service(TestPolicy);
            pin_mut!(handle);

            assert_ready_ok!(service.poll_ready(cx));
            let fut = service.call(NOT_CLONABLE);
            pin_mut!(fut);
            // Check that orig request has been issued.
            let req = assert_request_eq!(handle, NOT_CLONABLE);
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));

            // Check hedge has not been issued.
            assert_pending!(handle.as_mut().poll_request(cx));
            time.advance(Duration::from_millis(10));
            // Check fut is not ready.
            assert_pending!(fut.as_mut().poll(cx));
            // Check hedge has not been issued.
            assert_pending!(handle.as_mut().poll_request(cx));

            req.send_response("orig-done");
            // Check that fut gets orig response.
            assert_eq!(assert_ready_ok!(fut.as_mut().poll(cx)), "orig-done");
        });
    });
}

type Req = &'static str;
type Res = &'static str;
type Mock = tower_test::mock::Mock<Req, Res>;
type Handle = tower_test::mock::Handle<Req, Res>;

static NOT_RETRYABLE: &'static str = "NOT_RETRYABLE";
static NOT_CLONABLE: &'static str = "NOT_CLONABLE";

#[derive(Clone)]
struct TestPolicy;

impl tower_hedge::Policy<Req> for TestPolicy {
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

fn new_service<P: Policy<Req> + Clone>(policy: P) -> (Hedge<Mock, P>, Handle) {
    let (service, handle) = tower_test::mock::pair();

    let mock_latencies: [u64; 10] = [1, 1, 1, 1, 1, 1, 1, 1, 10, 10];

    let service = Hedge::new_with_mock_latencies(
        service,
        policy,
        10,
        0.9,
        Duration::from_secs(60),
        &mock_latencies,
    );
    (service, handle)
}
