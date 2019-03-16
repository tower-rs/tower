//! Various utility types and functions that are generally with Tower.

//! Tower is a library of modular and reusable components for building robust networking
//! clients and servers.
//!
//! This main crate is still a WIP.

pub mod builder;
#[macro_use]
extern crate futures;
#[cfg(test)]
extern crate tokio_mock_task;
extern crate tower_layer;
extern crate tower_service;
extern crate tower_service_util;

pub mod ext;

pub use ext::ServiceExt;
pub use tower_service_util::BoxService;
pub use tower_service_util::EitherService;
pub use tower_service_util::MakeConnection;
pub use tower_service_util::MakeService;
pub use tower_service_util::OptionService;
pub use tower_service_util::ServiceFn;
