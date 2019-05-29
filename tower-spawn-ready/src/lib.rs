#![doc(html_root_url = "https://docs.rs/tower-spawn-ready/0.1.0")]
#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

//! When an underlying service is not ready, drive it to readiness on a
//! background task.

pub mod error;
pub mod future;
mod layer;
mod service;

pub use crate::layer::SpawnReadyLayer;
pub use crate::service::SpawnReady;
