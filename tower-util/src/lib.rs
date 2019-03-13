//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;
#[cfg(test)]
extern crate tokio_mock_task;
extern crate tower_service;
extern crate tower_service_util;

pub use tower_service_util::boxed;
pub mod either;
pub mod ext;
pub use tower_service_util::option;
mod service_fn;

pub use tower_service_util::boxed::BoxService;
pub use either::EitherService;
pub use ext::ServiceExt;
pub use tower_service_util::MakeConnection;
pub use tower_service_util::MakeService;
pub use tower_service_util::option::OptionService;
pub use service_fn::ServiceFn;
