use std::task::Future;
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

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
    let send_response = assert_request_eq!(handle, "hello?");

    // Response is not ready
    with_task(|| {
        assert!(response.poll().unwrap().is_not_ready());
    });

    // Send the response
    send_response.send_response("yes?".into());

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

type Mock = mock::Mock<String, String>;
type Handle = mock::Handle<String, String>;

fn new_mock() -> (Mock, Handle) {
    mock::pair()
}

// Helper to run some code within context of a task
fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::lazy;
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
