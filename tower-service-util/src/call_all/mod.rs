//! `Stream<Item = Request>` + `Service<Request>` => `Stream<Item = Response>`.

mod common;
mod ordered;

pub use self::ordered::CallAll;

type Error = Box<::std::error::Error + Send + Sync>;
