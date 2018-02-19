//! Various utility types and functions that are generally with Tower.

extern crate futures;
extern crate tower;
extern crate tower_ready_service;

pub mod either;
pub mod option;
pub mod boxed;
mod service_fn;

pub use boxed::BoxService;
pub use either::EitherService;
pub use service_fn::{ServiceFn, NewServiceFn};
pub use option::OptionService;
