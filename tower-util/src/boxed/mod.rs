//! Trait object `Service` instances
//!
//! Dynamically dispatched `Service` objects allow for erasing the underlying
//! `Service` type and using the `Service` instances as opaque handles. This can
//! be useful when the service instance cannot be explicitly named for whatever
//! reason.
//!
//! There are two variants of service objects. `BoxService` requires both the
//! service and the response future to be `Send`. These values can move freely
//! across threads. `UnsyncBoxService` requires both the service and the
//! response future to remain on the current thread. This is useful for
//! representing services that are backed by `Rc` or other non-`Send` types.
//!
//! # Examples
//!
//! ```
//! # extern crate futures;
//! # extern crate tower_service;
//! # extern crate tower_util;
//! # use futures::*;
//! # use futures::future::FutureResult;
//! # use tower_service::Service;
//! # use tower_util::BoxService;
//! // Respond to requests using a closure. Since closures cannot be named,
//! // `ServiceFn` cannot be named either
//! pub struct ServiceFn<F> {
//!     f: F,
//! }
//!
//! impl<F> Service<String> for ServiceFn<F>
//! where F: Fn(String) -> String,
//! {
//!     type Response = String;
//!     type Error = ();
//!     type Future = FutureResult<String, ()>;
//!
//!     fn poll_ready(&mut self) -> Poll<(), ()> {
//!         Ok(().into())
//!     }
//!
//!     fn call(&mut self, request: String) -> FutureResult<String, ()> {
//!         future::ok((self.f)(request))
//!     }
//! }
//!
//! pub fn main() {
//!     let f = |mut request: String| {
//!         request.push_str(" response");
//!         request
//!     };
//!
//!     let service: BoxService<String, String, ()> =
//!         BoxService::new(ServiceFn { f });
//! # drop(service);
//! }
//! ```

mod sync;
mod unsync;

pub use self::{sync::BoxService, unsync::UnsyncBoxService};
