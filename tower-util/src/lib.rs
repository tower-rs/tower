#![doc(html_root_url = "https://docs.rs/tower-util/0.3.0-alpha.1")]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]
#![allow(elided_lifetimes_in_paths)]

//! Various utility types and functions that are generally with Tower.

mod boxed;
mod call_all;
mod either;
mod oneshot;
mod optional;
mod ready;
mod sealed;
mod service_fn;

/// Different ways to chain service layers.
pub mod layer;

pub use crate::{
    boxed::{BoxService, UnsyncBoxService},
    call_all::{CallAll, CallAllUnordered},
    either::Either,
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
