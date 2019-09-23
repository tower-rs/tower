#![doc(html_root_url = "https://docs.rs/tower-spawn-ready/0.3.0-alpha.1")]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]
#![allow(elided_lifetimes_in_paths)]

//! When an underlying service is not ready, drive it to readiness on a
//! background task.

pub mod error;
pub mod future;
mod layer;
mod make;
mod service;

pub use crate::layer::SpawnReadyLayer;
pub use crate::make::{MakeFuture, MakeSpawnReady};
pub use crate::service::SpawnReady;
