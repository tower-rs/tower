//! Various utility types and functions that are generally with Tower.

extern crate futures;
extern crate tower;

pub mod boxed;
mod clone;
pub mod either;
pub mod option;
mod service_fn;

pub use boxed::BoxService;
pub use clone::CloneService;
pub use either::EitherService;
pub use service_fn::NewServiceFn;
pub use option::OptionService;
