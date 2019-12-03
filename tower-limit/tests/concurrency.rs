#![cfg(feature = "broken")]

use futures_util::{future::poll_fn, pin_mut};
use tokio_test::{assert_pending, assert_ready, assert_ready_ok, block_on, task::MockTask};
use tower_limit::concurrency::ConcurrencyLimit;
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn basic_service_limit_functionality_with_poll_ready() {
    let mut task = MockTask::new();

    let (mut service, handle) = new_service(2);
    pin_mut!(handle);

    block_on(poll_fn(|cx| service.poll_ready(cx))).unwrap();
    let r1 = service.call("hello 1");

    block_on(poll_fn(|cx| service.poll_ready(cx))).unwrap();
    let r2 = service.call("hello 2");

    task.enter(|cx| {
        assert_pending!(service.poll_ready(cx));
    });

    assert!(!task.is_woken());

    // The request gets passed through
    assert_request_eq!(handle, "hello 1").send_response("world 1");

    // The next request gets passed through
    assert_request_eq!(handle, "hello 2").send_response("world 2");

    // There are no more requests
    task.enter(|cx| {
        assert_pending!(handle.as_mut().poll_request(cx));
    });

    assert_eq!(block_on(r1).unwrap(), "world 1");
    assert!(task.is_woken());

    // Another request can be sent
    task.enter(|cx| {
        assert_ready_ok!(service.poll_ready(cx));
    });

    let r3 = service.call("hello 3");

    task.enter(|cx| {
        assert_pending!(service.poll_ready(cx));
    });

    assert_eq!(block_on(r2).unwrap(), "world 2");

    // The request gets passed through
    assert_request_eq!(handle, "hello 3").send_response("world 3");

    assert_eq!(block_on(r3).unwrap(), "world 3");
}

#[test]
fn basic_service_limit_functionality_without_poll_ready() {
    let mut task = MockTask::new();

    let (mut service, handle) = new_service(2);
    pin_mut!(handle);

    assert_ready_ok!(task.enter(|cx| service.poll_ready(cx)));
    let r1 = service.call("hello 1");

    assert_ready_ok!(task.enter(|cx| service.poll_ready(cx)));
    let r2 = service.call("hello 2");

    assert_pending!(task.enter(|cx| service.poll_ready(cx)));

    // The request gets passed through
    assert_request_eq!(handle, "hello 1").send_response("world 1");

    assert!(!task.is_woken());

    // The next request gets passed through
    assert_request_eq!(handle, "hello 2").send_response("world 2");

    assert!(!task.is_woken());

    // There are no more requests
    assert_pending!(task.enter(|cx| handle.as_mut().poll_request(cx)));

    assert_eq!(block_on(r1).unwrap(), "world 1");

    assert!(task.is_woken());

    // One more request can be sent
    assert_ready_ok!(task.enter(|cx| service.poll_ready(cx)));
    let r4 = service.call("hello 4");

    assert_pending!(task.enter(|cx| service.poll_ready(cx)));

    assert_eq!(block_on(r2).unwrap(), "world 2");
    assert!(task.is_woken());

    // The request gets passed through
    assert_request_eq!(handle, "hello 4").send_response("world 4");

    assert_eq!(block_on(r4).unwrap(), "world 4");
}

#[test]
fn request_without_capacity() {
    let mut task = MockTask::new();

    let (mut service, _) = new_service(0);

    task.enter(|cx| {
        assert_pending!(service.poll_ready(cx));
    });
}

#[test]
fn reserve_capacity_without_sending_request() {
    let mut task = MockTask::new();

    let (mut s1, handle) = new_service(1);
    pin_mut!(handle);

    let mut s2 = s1.clone();

    // Reserve capacity in s1
    task.enter(|cx| {
        assert_ready_ok!(s1.poll_ready(cx));
    });

    // Service 2 cannot get capacity
    task.enter(|cx| {
        assert_pending!(s2.poll_ready(cx));
    });

    // s1 sends the request, then s2 is able to get capacity
    let r1 = s1.call("hello");

    assert_request_eq!(handle, "hello").send_response("world");

    task.enter(|cx| {
        assert_pending!(s2.poll_ready(cx));
    });

    block_on(r1).unwrap();

    task.enter(|cx| {
        assert_ready_ok!(s2.poll_ready(cx));
    });
}

#[test]
fn service_drop_frees_capacity() {
    let mut task = MockTask::new();

    let (mut s1, _handle) = new_service(1);

    let mut s2 = s1.clone();

    // Reserve capacity in s1
    assert_ready_ok!(task.enter(|cx| s1.poll_ready(cx)));

    // Service 2 cannot get capacity
    task.enter(|cx| {
        assert_pending!(s2.poll_ready(cx));
    });

    drop(s1);

    assert!(task.is_woken());
    assert_ready_ok!(task.enter(|cx| s2.poll_ready(cx)));
}

#[test]
fn response_error_releases_capacity() {
    let mut task = MockTask::new();

    let (mut s1, handle) = new_service(1);
    pin_mut!(handle);

    let mut s2 = s1.clone();

    // Reserve capacity in s1
    task.enter(|cx| {
        assert_ready_ok!(s1.poll_ready(cx));
    });

    // s1 sends the request, then s2 is able to get capacity
    let r1 = s1.call("hello");

    assert_request_eq!(handle, "hello").send_error("boom");

    block_on(r1).unwrap_err();

    task.enter(|cx| {
        assert_ready_ok!(s2.poll_ready(cx));
    });
}

#[test]
fn response_future_drop_releases_capacity() {
    let mut task = MockTask::new();

    let (mut s1, _handle) = new_service(1);

    let mut s2 = s1.clone();

    // Reserve capacity in s1
    task.enter(|cx| {
        assert_ready_ok!(s1.poll_ready(cx));
    });

    // s1 sends the request, then s2 is able to get capacity
    let r1 = s1.call("hello");

    task.enter(|cx| {
        assert_pending!(s2.poll_ready(cx));
    });

    drop(r1);

    task.enter(|cx| {
        assert_ready_ok!(s2.poll_ready(cx));
    });
}

#[test]
fn multi_waiters() {
    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();
    let mut task3 = MockTask::new();

    let (mut s1, _handle) = new_service(1);
    let mut s2 = s1.clone();
    let mut s3 = s1.clone();

    // Reserve capacity in s1
    task1.enter(|cx| assert_ready_ok!(s1.poll_ready(cx)));

    // s2 and s3 are not ready
    task2.enter(|cx| assert_pending!(s2.poll_ready(cx)));
    task3.enter(|cx| assert_pending!(s3.poll_ready(cx)));

    drop(s1);

    assert!(task2.is_woken());
    assert!(!task3.is_woken());

    drop(s2);

    assert!(task3.is_woken());
}

type Mock = mock::Mock<&'static str, &'static str>;
type Handle = mock::Handle<&'static str, &'static str>;

fn new_service(max: usize) -> (ConcurrencyLimit<Mock>, Handle) {
    let (service, handle) = mock::pair();
    let service = ConcurrencyLimit::new(service, max);
    (service, handle)
}
