use std::future::Future;
use crate::retry::Policy;

/// TODO docs
pub trait PolicyV2<Req, Res, E>: Sized {
    /// The [`Future`] type returned by [`Policy::retry`].
    type Future: Future<Output = Self>;

    /// Check the policy if a certain request should be retried.
    ///
    /// This method is passed a reference to the original request, and either
    /// the [`Service::Response`] or [`Service::Error`] from the inner service.
    ///
    /// If the request should **not** be retried, return `None`.
    ///
    /// If the request *should* be retried, return `Some` future of a new
    /// policy that would apply for the next request attempt.
    ///
    /// [`Service::Response`]: crate::Service::Response
    /// [`Service::Error`]: crate::Service::Error
    fn retry(&self, req: &Req, result: Result<Res, E>) -> Result<Result<Res, E>, Self::Future>;

    /// Tries to clone a request before being passed to the inner service.
    ///
    /// If the request cannot be cloned, return [`None`].
    fn clone_request(&self, req: &Req) -> Option<Req>;
}

impl<T, Req, Res, E> PolicyV2<Req, Res, E> for T where T: Policy<Req, Res, E> {
    type Future = T::Future;

    fn retry(&self, req: &Req, result: Result<Res, E>) -> Result<Result<Res, E>, Self::Future> {
        match Policy::<Req, Res, E>::retry(self, req, result.as_ref()) {
            Some(fut) => Err(fut),
            None => Ok(result)
        }
    }

    fn clone_request(&self, req: &Req) -> Option<Req> {
        Policy::<Req, Res, E>::clone_request(self, req)
    }
}
