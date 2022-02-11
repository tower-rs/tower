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
        // In some cases, this may be used with `bool` as the `Request` type, in
        // which case, clippy emits a warning. However, this can't be changed to
        // `assert!`, because the request type may *not* be `bool`...
        #[allow(clippy::bool_assert_comparison)]
        {
            assert_eq!(actual, $expect, $($arg)*);
        }
        send_response
    }};
}
