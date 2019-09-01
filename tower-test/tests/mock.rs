use std::future::Future;
use tower_service::Service;
use tower_test::{assert_request_eq, mock};
use futures_executor::block_on;
use futures_util::future::FutureExt;
use core::task::Context;

#[test]
fn single_request_ready() {
    let (mut mock, mut handle) = new_mock();

    // No pending requests
    with_task(|cx| {
        //assert!(handle.poll_request().poll_unpin(cx).is_pending());
    });

    // Issue a request
    /*assert!(mock.poll_ready().is_ready());
    let mut response = mock.call("hello?".into());

    // Get the request from the handle
    let send_response = assert_request_eq!(handle, "hello?");

    // Response is not ready
    with_task(|cx| {
        assert!(response.poll(cx).is_pending());
    });

    // Send the response
    send_response.send_response("yes?".into());

    assert_eq!(block_on(response.as_str()), "yes?");*/
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
    use futures_executor::block_on;

    block_on(lazy(|cx| Ok::<_, ()>(f(cx)))).unwrap()
}
