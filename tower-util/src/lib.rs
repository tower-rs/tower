//! Various utility types and functions that are generally with Tower.

extern crate futures;
extern crate tower;

pub mod either;
pub mod option;
mod service_fn;

pub use either::EitherService;
pub use service_fn::{ServiceFn, NewServiceFn};
pub use option::OptionService;
