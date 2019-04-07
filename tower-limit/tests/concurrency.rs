extern crate futures;
extern crate tokio_mock_task;
extern crate tower_limit;
extern crate tower_service;
#[macro_use]
extern crate tower_test;

use tower_limit::concurrency::ConcurrencyLimit;
use tower_service::Service;
use tower_test::mock;

use futures::future::{poll_fn, Future};
use tokio_mock_task::MockTask;

macro_rules! assert_ready {
    ($e:expr) => {{
        use futures::Async::*;
        match $e {
            Ok(Ready(v)) => v,
            Ok(NotReady) => panic!("not ready"),
            Err(e) => panic!("err = {:?}", e),
        }
    }}
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        use futures::Async::*;
        match $e {
            Ok(NotReady) => {}
            r => panic!("unexpected poll status = {:?}", r),
        }
    }}
}

#[test]
fn basic_service_limit_functionality_with_poll_ready() {
    let mut task = MockTask::new();

    let (mut service, mut handle) = new_service(2);

    poll_fn(|| service.poll_ready()).wait().unwrap();
    let r1 = service.call("hello 1");

    poll_fn(|| service.poll_ready()).wait().unwrap();
    let r2 = service.call("hello 2");

    task.enter(|| {
        assert!(service.poll_ready().unwrap().is_not_ready());
    });

    assert!(!task.is_notified());

    // The request gets passed through
    assert_request_eq!(handle, "hello 1")
        .send_response("world 1");

    // The next request gets passed through
    assert_request_eq!(handle, "hello 2")
        .send_response("world 2");

    // There are no more requests
    task.enter(|| {
        assert!(handle.poll_request().unwrap().is_not_ready());
    });

    assert_eq!(r1.wait().unwrap(), "world 1");
    assert!(task.is_notified());

    // Another request can be sent
    task.enter(|| {
        assert!(service.poll_ready().unwrap().is_ready());
    });

    let r3 = service.call("hello 3");

    task.enter(|| {
        assert!(service.poll_ready().unwrap().is_not_ready());
    });

    assert_eq!(r2.wait().unwrap(), "world 2");

    // The request gets passed through
    assert_request_eq!(handle, "hello 3")
        .send_response("world 3");

    assert_eq!(r3.wait().unwrap(), "world 3");
}

#[test]
fn basic_service_limit_functionality_without_poll_ready() {
    let mut task = MockTask::new();

    let (mut service, mut handle) = new_service(2);

    assert_ready!(service.poll_ready());
    let r1 = service.call("hello 1");

    assert_ready!(service.poll_ready());
    let r2 = service.call("hello 2");

    task.enter(|| {
        assert_not_ready!(service.poll_ready());
    });

    // The request gets passed through
    assert_request_eq!(handle, "hello 1")
        .send_response("world 1");

    assert!(!task.is_notified());

    // The next request gets passed through
    assert_request_eq!(handle, "hello 2")
        .send_response("world 2");

    assert!(!task.is_notified());

    // There are no more requests
    task.enter(|| {
        assert!(handle.poll_request().unwrap().is_not_ready());
    });

    assert_eq!(r1.wait().unwrap(), "world 1");

    assert!(task.is_notified());

    // One more request can be sent
    assert_ready!(service.poll_ready());
    let r4 = service.call("hello 4");

    task.enter(|| {
        assert_not_ready!(service.poll_ready());
    });

    assert_eq!(r2.wait().unwrap(), "world 2");
    assert!(task.is_notified());

    // The request gets passed through
    assert_request_eq!(handle, "hello 4")
        .send_response("world 4");

    assert_eq!(r4.wait().unwrap(), "world 4");
}

#[test]
fn request_without_capacity() {
    let mut task = MockTask::new();

    let (mut service, _) = new_service(0);

    task.enter(|| {
        assert_not_ready!(service.poll_ready());
    });
}

#[test]
fn reserve_capacity_without_sending_request() {
    let mut task = MockTask::new();

    let (mut s1, mut handle) = new_service(1);

    let mut s2 = s1.clone();

    // Reserve capacity in s1
    task.enter(|| {
        assert!(s1.poll_ready().unwrap().is_ready());
    });

    // Service 2 cannot get capacity
    task.enter(|| {
        assert!(s2.poll_ready().unwrap().is_not_ready());
    });

    // s1 sends the request, then s2 is able to get capacity
    let r1 = s1.call("hello");

    assert_request_eq!(handle, "hello")
        .send_response("world");

    task.enter(|| {
        assert!(s2.poll_ready().unwrap().is_not_ready());
    });

    r1.wait().unwrap();

    task.enter(|| {
        assert!(s2.poll_ready().unwrap().is_ready());
    });
}

#[test]
fn service_drop_frees_capacity() {
    let mut task = MockTask::new();

    let (mut s1, _handle) = new_service(1);

    let mut s2 = s1.clone();

    // Reserve capacity in s1
    assert_ready!(s1.poll_ready());

    // Service 2 cannot get capacity
    task.enter(|| {
        assert_not_ready!(s2.poll_ready());
    });

    drop(s1);

    assert!(task.is_notified());
    assert_ready!(s2.poll_ready());
}

#[test]
fn response_error_releases_capacity() {
    let mut task = MockTask::new();

    let (mut s1, mut handle) = new_service(1);

    let mut s2 = s1.clone();

    // Reserve capacity in s1
    task.enter(|| {
        assert_ready!(s1.poll_ready());
    });

    // s1 sends the request, then s2 is able to get capacity
    let r1 = s1.call("hello");

    assert_request_eq!(handle, "hello")
        .send_error("boom");

    r1.wait().unwrap_err();

    task.enter(|| {
        assert!(s2.poll_ready().unwrap().is_ready());
    });
}

#[test]
fn response_future_drop_releases_capacity() {
    let mut task = MockTask::new();

    let (mut s1, _handle) = new_service(1);

    let mut s2 = s1.clone();

    // Reserve capacity in s1
    task.enter(|| {
        assert_ready!(s1.poll_ready());
    });

    // s1 sends the request, then s2 is able to get capacity
    let r1 = s1.call("hello");

    task.enter(|| {
        assert_not_ready!(s2.poll_ready());
    });

    drop(r1);

    task.enter(|| {
        assert!(s2.poll_ready().unwrap().is_ready());
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
    task1.enter(|| assert_ready!(s1.poll_ready()));

    // s2 and s3 are not ready
    task2.enter(|| assert_not_ready!(s2.poll_ready()));
    task3.enter(|| assert_not_ready!(s3.poll_ready()));

    drop(s1);

    assert!(task2.is_notified());
    assert!(!task3.is_notified());

    drop(s2);

    assert!(task3.is_notified());
}

type Mock = mock::Mock<&'static str, &'static str>;
type Handle = mock::Handle<&'static str, &'static str>;

fn new_service(max: usize) -> (ConcurrencyLimit<Mock>, Handle) {
    let (service, handle) = mock::pair();
    let service = ConcurrencyLimit::new(service, max);
    (service, handle)
}
