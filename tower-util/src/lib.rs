//! Various utility types and functions that are generally with Tower.

extern crate futures;
extern crate tower_service;

#[macro_use]
mod macros {
    include! { concat!(env!("CARGO_MANIFEST_DIR"), "/../gen_errors.rs") }
}

pub mod either;
pub mod option;
pub mod boxed;
mod service_fn;



pub use boxed::BoxService;
pub use either::EitherService;
pub use service_fn::{NewServiceFn};
pub use option::OptionService;
