//! Various utility types and functions that are generally with Tower.

extern crate futures;
extern crate tower;

pub mod either;
pub mod option;
mod boxed;
mod service_fn;

pub use boxed::Boxed;
pub use either::EitherService;
pub use service_fn::NewServiceFn;
pub use option::OptionService;
