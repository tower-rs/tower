//! Trait aliases for Services that produce specific types of Responses.

mod make_connection;

pub mod make_service;

pub use self::make_connection::MakeConnection;
#[doc(inline)]
pub use self::make_service::MakeService;
