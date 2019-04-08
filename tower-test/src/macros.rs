/// Asserts that the mock handle receives a new request equal to the given
/// value.
///
/// On success, the [`SendResponse`] handle for the matched request is returned,
/// allowing the caller to respond to the request. On failure, the macro panics.
///
/// # Examples
///
/// ```rust
/// extern crate tower;
/// #[macro_use]
/// extern crate tower_test;
///
/// use tower::Service;
/// use tower_test::mock;
///
/// # fn main() {
/// let (mut mock, mut handle) = mock::pair();
///
/// assert!(mock.poll_ready().is_ok());
/// let _response = mock.call("hello");
///
/// assert_request_eq!(handle, "hello")
///     .send_response("world");
/// # }
/// ```
#[macro_export]
macro_rules! assert_request_eq {
    ($mock_handle:expr, $expect:expr) => {
        assert_request_eq!($mock_handle, $expect,)
    };
    ($mock_handle:expr, $expect:expr, $($arg:tt)*) => {{
        let (actual, send_response) = match $mock_handle.next_request() {
            Some(r) => r,
            None => panic!("expected a request but none was received."),
        };

        assert_eq!(actual, $expect, $($arg)*);
        send_response
    }};
}
