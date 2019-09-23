#![doc(html_root_url = "https://docs.rs/tower-buffer/0.3.0-alpha.1a")]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]
#![allow(elided_lifetimes_in_paths)]

//! Buffer requests when the inner service is out of capacity.
//!
//! Buffering works by spawning a new task that is dedicated to pulling requests
//! out of the buffer and dispatching them to the inner service. By adding a
//! buffer and a dedicated task, the `Buffer` layer in front of the service can
//! be `Clone` even if the inner service is not.

pub mod error;
pub mod future;
mod layer;
mod message;
mod service;
mod worker;

pub use crate::layer::BufferLayer;
pub use crate::service::Buffer;
pub use crate::worker::WorkerExecutor;
