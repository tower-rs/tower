// Allows refining features in the future without breaking backwards
// compatibility
#![cfg(feature = "full")]

//! Various utility types and functions that are generally with Tower.

#[macro_use]
extern crate futures;

extern crate tower_layer;
extern crate tower_service;
extern crate tower_util;

pub extern crate tower_balance as balance;
pub extern crate tower_buffer as buffer;
pub extern crate tower_discover as discover;
pub extern crate tower_filter as filter;
pub extern crate tower_in_flight_limit as in_flight_limit;
pub extern crate tower_load_shed as load_shed;
pub extern crate tower_rate_limit as rate_limit;
pub extern crate tower_reconnect as reconnect;
pub extern crate tower_retry as retry;
pub extern crate tower_timeout as timeout;

pub mod builder;
pub mod layer;
pub mod util;

pub use builder::ServiceBuilder;
pub use tower_service::Service;
pub use tower_util::MakeConnection;
pub use tower_util::MakeService;
pub use util::ServiceExt;
