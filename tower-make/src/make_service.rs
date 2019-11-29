use crate::sealed::Sealed;
use std::future::Future;
use std::task::{Context, Poll};
use tower_service::Service;

/// Creates new `Service` values.
///
/// Acts as a service factory. This is useful for cases where new `Service`
/// values must be produced. One case is a TCP server listener. The listener
/// accepts new TCP streams, obtains a new `Service` value using the
/// `MakeService` trait, and uses that new `Service` value to process inbound
/// requests on that new TCP stream.
///
/// This is essentially a trait alias for a `Service` of `Service`s.
pub trait MakeService<Target, Request>: Sealed<(Target, Request)> {
    /// Responses given by the service
    type Response;

    /// Errors produced by the service
    type Error;

    /// The `Service` value created by this factory
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;

    /// Errors produced while building a service.
    type MakeError;

    /// The future of the `Service` instance.
    type Future: Future<Output = Result<Self::Service, Self::MakeError>>;

    /// Returns `Ready` when the factory is able to create more services.
    ///
    /// If the service is at capacity, then `Poll::Pending` is returned and the task
    /// is notified when the service becomes ready again. This function is
    /// expected to be called while on a task.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::MakeError>>;

    /// Create and return a new service value asynchronously.
    fn make_service(&mut self, target: Target) -> Self::Future;
}

impl<M, S, Target, Request> Sealed<(Target, Request)> for M
where
    M: Service<Target, Response = S>,
    S: Service<Request>,
{
}

impl<M, S, Target, Request> MakeService<Target, Request> for M
where
    M: Service<Target, Response = S>,
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Service = S;
    type MakeError = M::Error;
    type Future = M::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::MakeError>> {
        Service::poll_ready(self, cx)
    }

    fn make_service(&mut self, target: Target) -> Self::Future {
        Service::call(self, target)
    }
}
