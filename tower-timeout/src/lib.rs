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
// mod layer;

// pub use crate::layer::TimeoutLayer;

use crate::{error::Error, future::ResponseFuture};
use tokio_timer::{clock, Delay};

use tower_service::Service;

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// Applies a timeout to requests.
#[derive(Debug)]
pub struct Timeout<S> {
    inner: S,
    timeout: Duration,
    // _pd: PhantomData<R>,
}

// ===== impl Timeout =====

impl<S> Timeout<S> {
    /// Creates a new Timeout
    pub fn new(inner: S, timeout: Duration) -> Self {
        Timeout { inner, timeout }
    }
}

impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request> + Unpin,
    S::Future: Unpin,
    S::Response: Send,
    S::Error: Send,
    Error: From<S::Error>,
{
    type Response = S::Response;
    type Error = Error;
    type Future = ResponseFuture<S::Future, S::Response, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = Pin::new(&mut self.inner).call(request);
        let sleep = Delay::new(clock::now() + self.timeout);
        ResponseFuture::new(response, sleep)
    }
}
