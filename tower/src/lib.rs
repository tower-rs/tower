//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;

extern crate tower_layer;
extern crate tower_service;
extern crate tower_util;

#[cfg(feature = "layers")]
extern crate tower_buffer;
#[cfg(feature = "layers")]
extern crate tower_filter;
#[cfg(feature = "layers")]
extern crate tower_in_flight_limit;
#[cfg(feature = "layers")]
extern crate tower_load_shed;
#[cfg(feature = "layers")]
extern crate tower_rate_limit;
#[cfg(feature = "layers")]
extern crate tower_retry;

#[cfg(feature = "make_service")]
extern crate tower_balance;
#[cfg(feature = "make_service")]
extern crate tower_discover;
#[cfg(feature = "make_service")]
extern crate tower_reconnect;

pub mod builder;
#[cfg(feature = "layers")]
pub mod layer;
#[cfg(feature = "make_service")]
pub mod make_service;
pub mod util;

pub use builder::ServiceBuilder;
pub use tower_service::Service;
pub use tower_util::MakeConnection;
pub use tower_util::MakeService;
pub use util::ServiceExt;
