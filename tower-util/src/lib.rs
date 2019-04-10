//! Various utility types and functions that are generally with Tower.
#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

mod boxed;
mod call_all;
mod either;
pub mod layer;
#[cfg(feature = "io")]
mod make_connection;
mod make_service;
mod oneshot;
mod optional;
mod ready;
mod sealed;
mod service_fn;

#[cfg(feature = "io")]
pub use crate::make_connection::MakeConnection;
pub use crate::{
    boxed::{BoxService, UnsyncBoxService},
    call_all::{CallAll, CallAllUnordered},
    either::Either,
    make_service::MakeService,
    oneshot::Oneshot,
    optional::Optional,
    ready::Ready,
    service_fn::{service_fn, ServiceFn},
};

pub mod error {
    //! Error types

    pub use crate::optional::error as optional;
}

pub mod future {
    //! Future types

    #[cfg(feature = "either")]
    pub use crate::either::future as either;
    pub use crate::optional::future as optional;
}
