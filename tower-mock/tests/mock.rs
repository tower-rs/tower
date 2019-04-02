extern crate futures;
extern crate tower_mock;
extern crate tower_service;

use tower_service::Service;

use futures::Future;

#[test]
fn single_request_ready() {
    let (mut mock, mut handle) = new_mock();

    // No pending requests
    with_task(|| {
        assert!(handle.poll_request().unwrap().is_not_ready());
    });

    // Issue a request
    assert!(mock.poll_ready().unwrap().is_ready());
    let mut response = mock.call("hello?".into());

    // Get the request from the handle
    let request = handle.next_request().unwrap();

    assert_eq!(request.as_str(), "hello?");

    // Response is not ready
    with_task(|| {
        assert!(response.poll().unwrap().is_not_ready());
    });

    // Send the response
    request.respond("yes?".into());

    assert_eq!(response.wait().unwrap().as_str(), "yes?");
}

#[test]
#[should_panic]
fn backpressure() {
    let (mut mock, mut handle) = new_mock();

    handle.allow(0);

    // Make sure the mock cannot accept more requests
    with_task(|| {
        assert!(mock.poll_ready().unwrap().is_not_ready());
    });

    // Try to send a request
    mock.call("hello?".into());
}

type Mock = tower_mock::Mock<String, String>;
type Handle = tower_mock::Handle<String, String>;

fn new_mock() -> (Mock, Handle) {
    Mock::new()
}

// Helper to run some code within context of a task
fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::{lazy, Future};
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
