#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
// #![deny(warnings)]
#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

//! Tower middleware for retrying "failed" requests.

pub mod budget;
pub mod future;
mod layer;
mod policy;

pub use crate::layer::RetryLayer;
pub use crate::policy::Policy;

use crate::future::ResponseFuture;
use futures::Poll;
use tower_service::Service;

/// Configure retrying requests of "failed" responses.
///
/// A `Policy` classifies what is a "failed" response.
#[derive(Clone, Debug)]
pub struct Retry<P, S> {
    policy: P,
    service: S,
}

// ===== impl Retry =====

impl<P, S> Retry<P, S> {
    /// Retry the inner service depending on this [`Policy`][Policy}.
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

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let cloned = self.policy.clone_request(&request);
        let future = self.service.call(request);

        ResponseFuture::new(cloned, self.clone(), future)
    }
}
