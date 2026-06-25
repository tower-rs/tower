#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![forbid(unsafe_code)]
#![allow(elided_lifetimes_in_paths)]
// `rustdoc::broken_intra_doc_links` is checked on CI

//! Mock [`Service`]s for use in tests.
//!
//! When testing code built on [tower], it is often useful to swap a real
//! service for one whose behaviour the test controls. [`mock::Mock`] is a
//! [`Service`] that does nothing on its own: each request it receives is handed
//! to a paired [`mock::Handle`], and the response is whatever the test chooses
//! to send back through that handle. This makes it possible to assert on the
//! exact requests a [`Service`] makes and to decide, per request, how to
//! respond (including failing).
//!
//! Create a [`Mock`]/[`Handle`] pair with [`mock::pair`], or use
//! [`mock::spawn()`] to get a [`mock::Spawn`] wrapper whose readiness can be
//! polled synchronously in tests (for example with [tokio-test]).
//!
//! # Example
//!
//! ```
//! use tokio_test::{assert_ready_ok, block_on};
//! use tower_service::Service;
//! use tower_test::mock;
//!
//! // Create a mock service together with the handle used to drive it.
//! let (mut service, mut handle) = mock::spawn();
//!
//! // A freshly-created mock is ready to accept a request.
//! assert_ready_ok!(service.poll_ready());
//!
//! // Calling the service sends the request to the handle and returns a future
//! // that resolves once the handle responds.
//! let response = service.call("hello");
//!
//! // Receive the request through the handle and reply to it.
//! let (request, send_response) =
//!     block_on(handle.next_request()).expect("the service was called");
//! assert_eq!(request, "hello");
//! send_response.send_response("world");
//!
//! // Now that a response has been sent, the service's response future resolves.
//! assert_eq!(block_on(response).unwrap(), "world");
//! ```
//!
//! [`Service`]: tower_service::Service
//! [tower]: https://docs.rs/tower
//! [tokio-test]: https://docs.rs/tokio-test
//! [`Mock`]: mock::Mock
//! [`Handle`]: mock::Handle

mod macros;
pub mod mock;
