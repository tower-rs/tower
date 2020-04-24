#![doc(html_root_url = "https://docs.rs/tower/0.3.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! `fn(Request) -> Future<Response>`
//!
//! Tower is a library of modular and reusable components for building
//! robust networking clients and servers.

#[cfg(feature = "balance")]
#[cfg_attr(docsrs, doc(cfg(feature = "balance")))]
pub mod balance;
#[cfg(feature = "buffer")]
#[cfg_attr(docsrs, doc(cfg(feature = "buffer")))]
pub mod buffer;
#[cfg(feature = "discover")]
#[cfg_attr(docsrs, doc(cfg(feature = "discover")))]
pub mod discover;
#[cfg(feature = "filter")]
#[cfg_attr(docsrs, doc(cfg(feature = "filter")))]
pub mod filter;
#[cfg(feature = "hedge")]
#[cfg_attr(docsrs, doc(cfg(feature = "hedge")))]
pub mod hedge;
#[cfg(feature = "limit")]
#[cfg_attr(docsrs, doc(cfg(feature = "limit")))]
pub mod limit;
#[cfg(feature = "load")]
#[cfg_attr(docsrs, doc(cfg(feature = "load")))]
pub mod load;
#[cfg(feature = "load-shed")]
#[cfg_attr(docsrs, doc(cfg(feature = "load-shed")))]
pub mod load_shed;
#[cfg(feature = "make")]
#[cfg_attr(docsrs, doc(cfg(feature = "make")))]
pub mod make;
#[cfg(feature = "ready-cache")]
#[cfg_attr(docsrs, doc(cfg(feature = "ready-cache")))]
pub mod ready_cache;
#[cfg(feature = "reconnect")]
#[cfg_attr(docsrs, doc(cfg(feature = "reconnect")))]
pub mod reconnect;
#[cfg(feature = "retry")]
#[cfg_attr(docsrs, doc(cfg(feature = "retry")))]
pub mod retry;
#[cfg(feature = "spawn-ready")]
#[cfg_attr(docsrs, doc(cfg(feature = "spawn-ready")))]
pub mod spawn_ready;
#[cfg(feature = "steer")]
#[cfg_attr(docsrs, doc(cfg(feature = "steer")))]
pub mod steer;
#[cfg(feature = "timeout")]
#[cfg_attr(docsrs, doc(cfg(feature = "timeout")))]
pub mod timeout;
#[cfg(feature = "util")]
#[cfg_attr(docsrs, doc(cfg(feature = "util")))]
pub mod util;

pub mod builder;
pub mod layer;

#[cfg(feature = "util")]
#[cfg_attr(docsrs, doc(cfg(feature = "util")))]
#[doc(inline)]
pub use self::util::{service_fn, ServiceExt};

#[doc(inline)]
pub use crate::builder::ServiceBuilder;
#[doc(inline)]
pub use tower_layer::Layer;
#[doc(inline)]
pub use tower_service::Service;

#[allow(unreachable_pub)]
mod sealed {
    pub trait Sealed<T> {}
}

/// Alias for a type-erased error type.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
