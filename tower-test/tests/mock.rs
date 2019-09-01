use core::task::Context;
use futures_executor::block_on;
use std::future::Future;
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn single_request_ready() {
    let (mut mock, handle) = new_mock();
    let mut handle = Box::pin(handle);

    // No pending requests
    with_task(|cx| {
        assert!(handle.as_mut().poll_request(cx).is_pending());
    });

    // Issue a request
    with_task(|cx| {
        assert!(mock.poll_ready(cx).is_ready());
    });

    let response = mock.call("hello?".into());
    let mut response = Box::pin(response);

    // Get the request from the handle
    let send_response = assert_request_eq!(handle.as_mut(), "hello?");

    // Response is not ready
    with_task(|cx| {
        assert!(response.as_mut().poll(cx).is_pending());
    });

    // Send the response
    send_response.send_response("yes?".into());

    assert_eq!(block_on(response.as_mut()).unwrap().as_str(), "yes?");
}

#[test]
#[should_panic]
fn backpressure() {
    let (mut mock, mut handle) = new_mock();

    handle.allow(0);

    // Make sure the mock cannot accept more requests
    with_task(|cx| {
        assert!(mock.poll_ready(cx).is_pending());
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
fn with_task<F: FnOnce(&mut Context<'_>) -> U, U>(f: F) -> U {
    use futures_util::future::lazy;

    block_on(lazy(|cx| Ok::<_, ()>(f(cx)))).unwrap()
}
