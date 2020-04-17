#![doc(html_root_url = "https://docs.rs/tower/0.3.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]

//! `fn(Request) -> Future<Response>`
//!
//! Tower is a library of modular and reusable components for building
//! robust networking clients and servers.

#[cfg(feature = "balance")]
pub mod balance;
#[cfg(feature = "buffer")]
pub mod buffer;
#[cfg(feature = "discover")]
pub mod discover;
#[cfg(feature = "filter")]
#[doc(hidden)] // not yet released
pub mod filter;
#[cfg(feature = "hedge")]
#[doc(hidden)] // not yet released
pub mod hedge;
#[cfg(feature = "limit")]
pub mod limit;
#[cfg(feature = "load")]
pub mod load;
#[cfg(feature = "load-shed")]
pub mod load_shed;
#[cfg(feature = "make")]
pub mod make;
#[cfg(feature = "ready-cache")]
pub mod ready_cache;
#[cfg(feature = "reconnect")]
pub mod reconnect;
#[cfg(feature = "retry")]
pub mod retry;
#[cfg(feature = "spawn-ready")]
pub mod spawn_ready;
#[cfg(feature = "steer")]
pub mod steer;
#[cfg(feature = "timeout")]
pub mod timeout;
#[cfg(feature = "util")]
pub mod util;

pub mod builder;
pub mod layer;

#[cfg(feature = "util")]
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
