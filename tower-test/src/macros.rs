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
/// use futures_executor::block_on;
///
/// fn with_task<F: FnOnce(&mut Context<'_>) -> U, U>(f: F) -> U {
///     use futures_util::future::lazy;
///
///     block_on(lazy(|cx| Ok::<_, ()>(f(cx)))).unwrap()
/// }
///
/// # fn main() {
/// let (mock, handle) = mock::pair();
/// let mut mock = Box::pin(mock);
/// let mut handle = Box::pin(handle);
///
/// with_task(|cx|{
///     assert!(mock.as_mut().poll_ready(cx).is_ready());
/// });
///
/// let _response = mock.as_mut().call("hello");
///
/// assert_request_eq!(handle.as_mut(), "hello")
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
