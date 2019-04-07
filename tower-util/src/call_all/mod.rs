//! `Stream<Item = Request>` + `Service<Request>` => `Stream<Item = Response>`.

mod common;
mod ordered;
mod unordered;

pub use self::ordered::CallAll;
pub use self::unordered::CallAllUnordered;

type Error = Box<dyn ::std::error::Error + Send + Sync>;
