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

pub use self::layer::BufferLayer;
pub use self::service::Buffer;
