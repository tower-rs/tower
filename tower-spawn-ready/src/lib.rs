#![doc(html_root_url = "https://docs.rs/tower-spawn-ready/0.3.0")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]

//! When an underlying service is not ready, drive it to readiness on a
//! background task.

pub mod future;
mod layer;
mod make;
mod service;

pub use crate::layer::SpawnReadyLayer;
pub use crate::make::{MakeFuture, MakeSpawnReady};
pub use crate::service::SpawnReady;

/// Errors produced by `SpawnReady`.
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;
