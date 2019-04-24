use futures::Future;

/// A "retry policy" to classify if a request should be retried.
///
/// # Example
///
/// ```
/// extern crate futures;
/// extern crate tower_retry;
///
/// use tower_retry::Policy;
///
/// type Req = String;
/// type Res = String;
///
/// struct Attempts(usize);
///
/// impl<E> Policy<Req, Res, E> for Attempts {
///     type Future = futures::future::FutureResult<Self, ()>;
///
///     fn retry(&self, req: &Req, result: Result<&Res, &E>) -> Option<Self::Future> {
///         match result {
///             Ok(_) => {
///                 // Treat all `Response`s as success,
///                 // so don't retry...
///                 None
///             },
///             Err(_) => {
///                 // Treat all errors as failures...
///                 // But we limit the number of attempts...
///                 if self.0 > 0 {
///                     // Try again!
///                     Some(futures::future::ok(Attempts(self.0 - 1)))
///                 } else {
///                     // Used all our attempts, no retry...
///                     None
///                 }
///             }
///         }
///     }
///
///     fn clone_request(&self, req: &Req) -> Option<Req> {
///         Some(req.clone())
///     }
/// }
/// ```
pub trait Policy<Req, Res, E>: Sized {
    /// The `Future` type returned by `Policy::retry()`.
    type Future: Future<Item = Self, Error = ()>;
    /// Check the policy if a certain request should be retried.
    ///
    /// This method is passed a reference to the original request, and either
    /// the `Service::Response` or `Service::Error` from the inner service.
    ///
    /// If the request should **not** be retried, return `None`.
    ///
    /// If the request *should* be retried, return `Some` future of a new
    /// policy that would apply for the next request attempt.
    ///
    /// If the returned `Future` errors, the request will **not** be retried
    /// after all.
    fn retry(&self, req: &Req, result: Result<&Res, &E>) -> Option<Self::Future>;
    /// Tries to clone a request before being passed to the inner service.
    ///
    /// If the request cannot be cloned, return `None`.
    fn clone_request(&self, req: &Req) -> Option<Req>;
}
