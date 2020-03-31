//! `Stream<Item = Request>` + `Service<Request>` => `Stream<Item = Response>`.

mod common;
mod ordered;
mod unordered;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::{ordered::CallAll, unordered::CallAllUnordered};

type Error = Box<dyn std::error::Error + Send + Sync>;
