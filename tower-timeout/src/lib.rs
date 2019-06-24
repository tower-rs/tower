#![feature(async_await)]
#![doc(html_root_url = "https://docs.rs/tower-timeout/0.1.0")]
#![deny(missing_debug_implementations, missing_docs, rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]
#![cfg_attr(test, deny(warnings))]

//! Tower middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

pub mod error;
pub mod future;
mod layer;

pub use crate::layer::TimeoutLayer;

use crate::{error::Error, future::ResponseFuture};
use tokio_timer::{clock, Delay};

use tower_service::Service;

use std::time::Duration;

/// Applies a timeout to requests.
#[derive(Debug)]
pub struct Timeout<'a, T> {
    inner: &'a mut T,
    timeout: Duration,
}

// ===== impl Timeout =====

impl<'a, T> Timeout<'a, T> {
    /// Creates a new Timeout
    pub fn new(inner: &'a mut T, timeout: Duration) -> Self {
        Timeout { inner, timeout }
    }
}

impl<'a, S, Request: 'a> Service<'a, Request> for Timeout<'a, S>
where
    S: Service<'a, Request> + Send + Sync,
    S::Future: Unpin,
    S::Response: Send,
    S::Error: Send,
    Error: From<S::Error>,
{
    type Response = S::Response;
    type Error = Error;
    type Future = ResponseFuture<S::Future, S::Response, S::Error>;

    // fn poll_ready(&mut self) -> Poll<(), Self::Error> {
    //     self.inner.poll_ready().map_err(Into::into)
    // }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = self.inner.call(request);
        let sleep = Delay::new(clock::now() + self.timeout);
        ResponseFuture::new(response, sleep)
    }
}
