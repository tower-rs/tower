//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;
extern crate tokio_io;
#[cfg(test)]
extern crate tokio_mock_task;
extern crate tower_service;

pub mod boxed;
pub mod either;
pub mod ext;
mod make_connection;
mod make_service;
pub mod option;
mod service_fn;

pub use boxed::BoxService;
pub use either::EitherService;
pub use ext::ServiceExt;
pub use make_connection::MakeConnection;
pub use make_service::MakeService;
pub use option::OptionService;
pub use service_fn::ServiceFn;
