use futures_util::pin_mut;
use std::future::Future;
use tokio_test::{assert_pending, assert_ready};
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn single_request_ready() {
    mock::task_fn::<String, String, _>(|cx, mock, handle| {
        // No pending requests
        assert!(handle.as_mut().poll_request(cx).is_pending());

        // Issue a request
        assert_ready!(mock.poll_ready(cx)).unwrap();

        let response = mock.call("hello?".into());
        pin_mut!(response);

        // Get the request from the handle
        let send_response = assert_request_eq!(handle, "hello?");

        // Response is not ready
        assert_pending!(response.as_mut().poll(cx));

        // Send the response
        send_response.send_response("yes?".into());

        assert_eq!(tokio_test::block_on(response).unwrap().as_str(), "yes?");
    });
}

#[test]
#[should_panic]
fn backpressure() {
    mock::task_fn::<String, String, _>(|cx, mock, handle| {
        handle.allow(0);

        // Make sure the mock cannot accept more requests
        assert_pending!(mock.poll_ready(cx));

        // Try to send a request
        mock.call("hello?".into());
    });
}
