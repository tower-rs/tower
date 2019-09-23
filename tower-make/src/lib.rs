#![doc(html_root_url = "https://docs.rs/tower-make/0.3.0-alpha.2")]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]

//! Trait aliases for Services that produce specific types of Responses.

#[cfg(feature = "io")]
mod make_connection;
mod make_service;

#[cfg(feature = "io")]
pub use crate::make_connection::MakeConnection;
pub use crate::make_service::MakeService;

mod sealed {
    pub trait Sealed<T> {}
}
