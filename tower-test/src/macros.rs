/// Asserts that the mock handle receives a new request equal to the given
/// value.
///
/// On success, the [`SendResponse`] handle for the matched request is returned,
/// allowing the caller to respond to the request. On failure, the macro panics.
///
/// # Examples
///
/// ```rust
/// #[macro_use]
/// extern crate tower_test;
/// extern crate tower_service;
///
/// use tower_service::Service;
/// use tower_test::mock;
/// use std::task::{Poll, Context};
/// use tokio_test::assert_ready;
/// use futures_util::pin_mut;
///
/// # fn main() {
/// mock::task_fn(|cx, mock, handle|{
///     assert_ready!(mock.poll_ready(cx));
///
///     let _response = mock.call("hello");
///
///     assert_request_eq!(handle, "hello")
///         .send_response("world");
/// });
/// # }
/// ```
#[macro_export]
macro_rules! assert_request_eq {
    ($mock_handle:expr, $expect:expr) => {
        assert_request_eq!($mock_handle, $expect,)
    };
    ($mock_handle:expr, $expect:expr, $($arg:tt)*) => {{
        let (actual, send_response) = match $mock_handle.as_mut().next_request() {
            Some(r) => r,
            None => panic!("expected a request but none was received."),
        };

        assert_eq!(actual, $expect, $($arg)*);
        send_response
    }};
}
