//! Various utility types and functions that are generally with Tower.

#[cfg(feature = "either")]
extern crate either as _either;
extern crate futures;
#[cfg(feature = "io")]
extern crate tokio_io;
extern crate tower_service;

pub mod boxed;
#[cfg(feature = "either")]
pub mod either;
pub mod option;

#[cfg(feature = "io")]
mod make_connection;
mod make_service;
mod sealed;
mod service_fn;

#[cfg(feature = "io")]
pub use crate::make_connection::MakeConnection;
pub use crate::make_service::MakeService;
pub use crate::service_fn::ServiceFn;

pub use tower_service::Service;
