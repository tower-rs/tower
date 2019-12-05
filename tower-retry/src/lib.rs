#![doc(html_root_url = "https://docs.rs/tower-retry/0.3.0")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]

//! Tower middleware for retrying "failed" requests.

pub mod budget;
pub mod future;
mod layer;
mod policy;

pub use crate::layer::RetryLayer;
pub use crate::policy::Policy;

use crate::future::ResponseFuture;
use pin_project::pin_project;
use std::task::{Context, Poll};
use tower_service::Service;

/// Configure retrying requests of "failed" responses.
///
/// A `Policy` classifies what is a "failed" response.
#[pin_project]
#[derive(Clone, Debug)]
pub struct Retry<P, S> {
    #[pin]
    policy: P,
    service: S,
}

// ===== impl Retry =====

impl<P, S> Retry<P, S> {
    /// Retry the inner service depending on this `Policy`.
    pub fn new(policy: P, service: S) -> Self {
        Retry { policy, service }
    }
}

impl<P, S, Request> Service<Request> for Retry<P, S>
where
    P: Policy<Request, S::Response, S::Error> + Clone,
    S: Service<Request> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<P, S, Request>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // NOTE: the Future::poll impl for ResponseFuture assumes that Retry::poll_ready is
        // equivalent to Ready.service.poll_ready. If this ever changes, that code must be updated
        // as well.
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let cloned = self.policy.clone_request(&request);
        let future = self.service.call(request);

        ResponseFuture::new(cloned, self.clone(), future)
    }
}
