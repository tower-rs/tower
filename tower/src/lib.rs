//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;
extern crate tokio;
#[cfg(test)]
extern crate tokio_mock_task;
extern crate tower_buffer;
extern crate tower_service;
extern crate tower_service_util;

pub mod blocking;
pub mod util;

pub use blocking::BlockingService;
pub use tower_service::Service;
pub use tower_service_util::MakeConnection;
pub use tower_service_util::MakeService;
pub use util::ServiceExt;
