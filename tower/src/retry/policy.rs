use std::future::Future;

/// A "retry policy" to classify if a request should be retried.
///
/// # Example
///
/// ```
/// use tower::retry::Policy;
/// use futures_util::future;
///
/// type Req = String;
/// type Res = String;
///
/// struct Attempts(usize);
///
/// impl<E> Policy<Req, Res, E> for Attempts {
///     type Future = future::Ready<Self>;
///
///     fn retry(&self, req: &mut Req, result: &mut Result<Res, E>) -> Option<Self::Future> {
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
///                     Some(future::ready(Attempts(self.0 - 1)))
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
    /// The [`Future`] type returned by [`Policy::retry`].
    type Future: Future<Output = Option<Req>>;

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
    /// ## Mutating Requests
    ///
    /// The policy MAY chose to mutate the `req`: if the request is mutated, the
    /// mutated request will be sent to the inner service in the next retry.
    /// This can be helpful for use cases like tracking the retry count in a
    /// header.
    ///
    /// ## Mutating Results
    ///
    /// The policy MAY chose to mutate the result. This enables the retry
    /// policy to convert a failure into a success and vice versa. For example,
    /// if the policy is used to poll while waiting for a state change, the
    /// policy can switch the result to emit a specific error when retries are
    /// exhausted.
    ///
    /// The policy can also record metadata on the request to include
    /// information about the number of retries required or to record that a
    /// failure failed after exhausting all retries.
    ///
    /// [`Service::Response`]: crate::Service::Response
    /// [`Service::Error`]: crate::Service::Error
    fn retry(&mut self, result: &mut Result<Res, E>) -> Self::Future;

    /// Called before a request is sent.
    fn pre_request(&mut self, _req: &mut Req) {}
}
