// Allows refining features in the future without breaking backwards
// compatibility
#![cfg(feature = "full")]
#![deny(missing_docs, rust_2018_idioms)]

//! Various utility types and functions that are generally with Tower.

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

pub mod builder;
pub mod layer;
pub mod util;

pub use crate::{builder::ServiceBuilder, util::ServiceExt};
pub use tower_service::Service;
pub use tower_util::{service_fn, MakeConnection, MakeService};
