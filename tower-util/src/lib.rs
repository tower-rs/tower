#![doc(html_root_url = "https://docs.rs/tower-util/0.3.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]

//! Various utility types and functions that are generally with Tower.

mod boxed;
#[cfg(feature = "call-all")]
mod call_all;
mod either;
mod oneshot;
mod optional;
mod ready;
mod service_fn;

pub use crate::{
    boxed::{BoxService, UnsyncBoxService},
    either::Either,
    oneshot::Oneshot,
    optional::Optional,
    ready::{Ready, ReadyAnd, ReadyOneshot},
    service_fn::{service_fn, ServiceFn},
};

#[cfg(feature = "call-all")]
pub use crate::call_all::{CallAll, CallAllUnordered};

#[doc(hidden)]
pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub mod error {
    //! Error types

    pub use crate::optional::error as optional;
}

pub mod future {
    //! Future types

    pub use crate::optional::future as optional;
}

/// An extension trait for `Service`s that provides a variety of convenient
/// adapters
pub trait ServiceExt<Request>: tower_service::Service<Request> {
    /// Resolves when the service is ready to accept a request.
    #[deprecated(since = "0.3.1", note = "prefer `ready_and` which yields the service")]
    fn ready(&mut self) -> Ready<'_, Self, Request>
    where
        Self: Sized,
    {
        #[allow(deprecated)]
        Ready::new(self)
    }

    /// Yields a mutable reference to the service when it is ready to accept a request.
    fn ready_and(&mut self) -> ReadyAnd<'_, Self, Request>
    where
        Self: Sized,
    {
        ReadyAnd::new(self)
    }

    /// Yields the service when it is ready to accept a request.
    fn ready_oneshot(self) -> ReadyOneshot<Self, Request>
    where
        Self: Sized,
    {
        ReadyOneshot::new(self)
    }

    /// Consume this `Service`, calling with the providing request once it is ready.
    fn oneshot(self, req: Request) -> Oneshot<Self, Request>
    where
        Self: Sized,
    {
        Oneshot::new(self, req)
    }

    /// Process all requests from the given `Stream`, and produce a `Stream` of their responses.
    ///
    /// This is essentially `Stream<Item = Request>` + `Self` => `Stream<Item = Response>`. See the
    /// documentation for [`CallAll`](struct.CallAll.html) for details.
    #[cfg(feature = "call-all")]
    fn call_all<S>(self, reqs: S) -> CallAll<Self, S>
    where
        Self: Sized,
        Self::Error: Into<Error>,
        S: futures_core::Stream<Item = Request>,
    {
        CallAll::new(self, reqs)
    }
}

impl<T: ?Sized, Request> ServiceExt<Request> for T where T: tower_service::Service<Request> {}
