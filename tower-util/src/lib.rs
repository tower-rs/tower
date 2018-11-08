//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;
extern crate tower_service;

pub mod boxed;
pub mod either;
pub mod ext;
pub mod option;
mod service_fn;

pub use boxed::BoxService;
pub use either::EitherService;
pub use ext::ServiceExt;
pub use option::OptionService;
pub use service_fn::ServiceFn;
