//! Various utility types and functions that are generally with Tower.

extern crate futures;
#[cfg(feature = "io")]
extern crate tokio_io;
extern crate tower_service;

pub mod boxed;
pub mod option;

#[cfg(feature = "io")]
mod make_connection;
mod make_service;
mod sealed;

#[cfg(feature = "io")]
pub use crate::make_connection::MakeConnection;
pub use crate::make_service::MakeService;

pub use tower_service::Service;
