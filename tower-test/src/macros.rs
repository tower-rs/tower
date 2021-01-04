/// Asserts that the mock handle receives a new request equal to the given
/// value.
///
/// On success, the [`SendResponse`] handle for the matched request is returned,
/// allowing the caller to respond to the request. On failure, the macro panics.
///
/// # Examples
///
/// ```rust
/// use tower_service::Service;
/// use tower_test::{mock, assert_request_eq};
/// use tokio_test::assert_ready;
///
/// # async fn test() {
/// let (mut service, mut handle) = mock::spawn();
///
/// assert_ready!(service.poll_ready());
///
/// let response = service.call("hello");
///
/// assert_request_eq!(handle, "hello").send_response("world");
///
/// assert_eq!(response.await.unwrap(), "world");
/// # }
/// ```
/// [`SendResponse`]: crate::mock::SendResponse
#[macro_export]
macro_rules! assert_request_eq {
    ($mock_handle:expr, $expect:expr) => {
        assert_request_eq!($mock_handle, $expect,)
    };
    ($mock_handle:expr, $expect:expr, $($arg:tt)*) => {{
        let (actual, send_response) = match $mock_handle.next_request().await {
            Some(r) => r,
            None => panic!("expected a request but none was received."),
        };

        assert_eq!(actual, $expect, $($arg)*);
        send_response
    }};
}
