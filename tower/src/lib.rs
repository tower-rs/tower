// Allows refining features in the future without breaking backwards
// compatibility
#![cfg(feature = "full")]

//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;

extern crate tower_layer;
extern crate tower_service;
extern crate tower_util;

extern crate tower_balance;
extern crate tower_buffer;
extern crate tower_discover;
extern crate tower_filter;
extern crate tower_in_flight_limit;
extern crate tower_load_shed;
extern crate tower_rate_limit;
extern crate tower_reconnect;
extern crate tower_retry;
extern crate tower_timeout;

pub mod balance;
pub mod buffer;
pub mod builder;
pub mod discover;
pub mod filter;
pub mod in_flight_limit;
pub mod layer;
pub mod load_shed;
pub mod make_service;
pub mod rate_limit;
pub mod reconnect;
pub mod retry;
pub mod timeout;
pub mod util;

pub use builder::ServiceBuilder;
pub use tower_service::Service;
pub use tower_util::MakeConnection;
pub use tower_util::MakeService;
pub use util::ServiceExt;
