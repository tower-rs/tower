//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;
#[cfg(feature = "io")]
extern crate tokio_io;
extern crate tower_layer;
extern crate tower_service;

mod boxed;
mod call_all;
mod either;
pub mod layer;
#[cfg(feature = "io")]
mod make_connection;
mod make_service;
mod oneshot;
mod option;
mod ready;
mod sealed;
mod service_fn;

pub use crate::boxed::{BoxService, UnsyncBoxService};
pub use crate::call_all::{CallAll, CallAllUnordered};
pub use crate::either::Either;
#[cfg(feature = "io")]
pub use crate::make_connection::MakeConnection;
pub use crate::make_service::MakeService;
pub use crate::oneshot::Oneshot;
pub use crate::option::OptionService;
pub use crate::ready::Ready;
pub use crate::service_fn::ServiceFn;

pub mod error {
    //! Error types

    pub use crate::option::error as option;
}

pub mod future {
    //! Future types

    #[cfg(feature = "either")]
    pub use crate::either::future as either;
    pub use crate::option::future as option;
}
