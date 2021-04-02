//! Middleware that executes a service on a remote tokio executor.
//!
//! When multiple executors are running it's sometimes desirable to have a service execute on a
//! particular one, for example the one with the most worker threads or the one that supports
//! blocking operations via [`task::block_in_place`].
//!
//! This module allows you to do that by placing the service behind a multi-producer, single-
//! consumer channel and spawning it onto an executor. The service then processes any requests sent
//! through the channel, spawning the futures covering their execution onto the remote executor.
//!
//! The result of a request is then transparently sent through another channel back to the client.
//!
//! [`task::block_in_place`]: tokio::task::block_in_place

mod layer;
mod service;
mod spawn;

pub use self::{layer::*, service::Remote};

/// Future types for the [`Remote`] middleware.
pub mod future {
    #[doc(inline)]
    pub use super::service::RemoteFuture;
}
