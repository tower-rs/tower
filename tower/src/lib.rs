#![doc(html_root_url = "https://docs.rs/tower/0.3.1")]
// Allows refining features in the future without breaking backwards
// compatibility
#![cfg(feature = "full")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]

//! `fn(Request) -> Future<Response>`
//!
//! Tower is a library of modular and reusable components for building
//! robust networking clients and servers.

#[doc(inline)]
pub use tower_buffer as buffer;
#[doc(inline)]
pub use tower_discover as discover;
#[doc(inline)]
pub use tower_limit as limit;
#[doc(inline)]
pub use tower_load_shed as load_shed;
#[doc(inline)]
pub use tower_retry as retry;
#[doc(inline)]
pub use tower_timeout as timeout;
// pub use tower_layer as layer;
#[doc(inline)]
pub use tower_layer as layer;

pub mod builder;
pub mod util;

pub use crate::{builder::ServiceBuilder, util::ServiceExt};
pub use tower_service::Service;
pub use tower_util::service_fn;
