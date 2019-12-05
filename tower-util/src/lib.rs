#![doc(html_root_url = "https://docs.rs/tower-util/0.3.0")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
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
