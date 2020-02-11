use std::future::Future;

/// A "retry policy" to classify if a request should be retried.
pub trait Policy<Req, Res, E>: Sized {
    /// The `Future` type returned by `Policy::retry()`.
    type Future: Future<Output = Self>;
    /// Check the policy if a certain request should be retried.
    ///
    /// This method is passed a reference to the original request, and either
    /// the `Service::Response` or `Service::Error` from the inner service.
    ///
    /// If the request should **not** be retried, return `None`.
    ///
    /// If the request *should* be retried, return `Some` future of a new
    /// policy that would apply for the next request attempt.
    fn retry(&self, req: &Req, result: Result<&Res, &E>) -> Option<Self::Future>;
    /// Tries to clone a request before being passed to the inner service.
    ///
    /// If the request cannot be cloned, return `None`.
    fn clone_request(&self, req: &Req) -> Option<Req>;
}
