//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;
#[cfg(test)]
extern crate tokio_mock_task;
extern crate tower_service;
extern crate tower_service_util;

pub mod ext;

pub use tower_service_util::EitherService;
pub use ext::ServiceExt;
pub use tower_service_util::ServiceFn;
pub use tower_service_util::BoxService;
pub use tower_service_util::OptionService;
pub use tower_service_util::MakeConnection;
pub use tower_service_util::MakeService;
