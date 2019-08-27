/// Trait aliases for Services that produce specific types of Responses.

#[cfg(feature = "io")]
mod make_connection;
mod make_service;

#[cfg(feature = "io")]
pub use crate::make_connection::MakeConnection;
pub use crate::make_service::MakeService;

mod sealed {
    pub trait Sealed<T> {}
}
