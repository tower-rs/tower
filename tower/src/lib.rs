#![doc(html_root_url = "https://docs.rs/tower/0.4.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(broken_intra_doc_links)]
#![allow(elided_lifetimes_in_paths, clippy::type_complexity)]
#![cfg_attr(test, allow(clippy::float_cmp))]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! `async fn(Request) -> Result<Response, Error>`
//!
//! # Overview
//!
//! Tower is a library of modular and reusable components for building
//! robust networking clients and servers.
//!
//! Tower provides a simple core abstraction, the [`Service`] trait, which
//! represents an asynchronous function taking a request and returning either a
//! response or an error. This abstraction can be used to model both clients and
//! servers.
//!
//! Generic components, like [timeouts], [rate limiting], and [load balancing],
//! can be modeled as [`Service`]s that wrap some inner service and apply
//! additional behavior before or after the inner service is called. This allows
//! implementing these components in a protocol-agnostic, composable way. Typically,
//! such services are referred to as _middleware_.
//!
//! An additional abstraction, the [`Layer`] trait, is used to compose
//! middleware with [`Service`]s. If a [`Service`] can be thought of as an
//! asynchronous function from a request type to a response type, a [`Layer`] is
//! a function taking a [`Service`] of one type and returning a [`Service`] of a
//! different type. The [`ServiceBuilder`] type is used to add middleware to a
//! service by composing it with multiple multiple [`Layer`]s.
//!
//! ## The Tower Ecosystem
//!
//! Tower is made up of the following crates:
//!
//! * [`tower`] (this crate)
//! * [`tower-service`]
//! * [`tower-layer`]
//! * [`tower-test`]
//!
//! Since the [`Service`] and [`Layer`] traits are important integration points
//! for all libraries using Tower, they are kept as stable as possible, and
//! breaking changes are made rarely. Therefore, they are defined in separate
//! crates, [`tower-service`] and [`tower-layer`]. This crate contains
//! re-exports of those core traits, implementations of commonly-used
//! middleware, and [utilities] for working with [`Service`]s and [`Layer`]s.
//! Finally, the [`tower-test`] crate provides tools for testing programs using
//! Tower.
//!
//! # Usage
//!
//! The various middleware implementations provided by this crate are feature
//! flagged, so that users can only compile the parts of Tower they need. By
//! default, all the optional middleware are disabled.
//!
//! To get started using all of Tower's optional middleware, add this to your
//! `Cargo.toml`:
//!
//! ```toml
//! tower = { version = "0.4", features = ["full"] }
//! ```
//!
//! Alternatively, you can only enable some features. For example, to enable
//! only the [`retry`] and [`timeout`] middleware, write:
//!
//! ```toml
//! tower = { version = "0.4", features = ["retry", "timeout"] }
//! ```
//!
//! See [here](#modules) for a complete list of all middleware provided by
//! Tower.
//!
//! [`Service`]: crate::Service
//! [`Layer]: crate::Layer
//! [timeouts]: crate::timeout
//! [rate limiting]: crate::limit::rate
//! [load balancing]: crate::balance
//! [`ServiceBuilder`]: crate::ServiceBuilder
//! [utilities]: crate::ServiceExt
//! [`tower`]: https://crates.io/crates/tower
//! [`tower-service`]: https://crates.io/crates/tower-service
//! [`tower-layer`]: https://crates.io/crates/tower-layer
//! [`tower-test`]: https://crates.io/crates/tower-test
//! [`retry`]: crate::retry
//! [`timeout`]: crate::timeout
#[macro_use]
pub(crate) mod macros;
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
#[cfg(feature = "make")]
#[cfg_attr(docsrs, doc(cfg(feature = "make")))]
#[doc(inline)]
pub use crate::make::MakeService;
#[doc(inline)]
pub use tower_layer::Layer;
#[doc(inline)]
pub use tower_service::Service;
#[cfg(any(feature = "buffer", feature = "limit"))]
mod semaphore;

#[allow(unreachable_pub)]
mod sealed {
    pub trait Sealed<T> {}
}

/// Alias for a type-erased error type.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
