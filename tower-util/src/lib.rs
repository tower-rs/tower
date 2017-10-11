//! Various utility types and functions that are generally with Tower.

extern crate futures;
extern crate tower;

mod either;
mod option;
mod service_fn;

pub use service_fn::{ServiceFn, NewServiceFn};
