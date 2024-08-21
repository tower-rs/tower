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
///     type Future = future::Ready<()>;
///
///     fn retry(&mut self, req: &mut Req, result: &mut Result<Res, E>) -> Option<Self::Future> {
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
///                     self.0 -= 1;
///                     Some(future::ready(()))
///                 } else {
///                     // Used all our attempts, no retry...
///                     None
///                 }
///             }
///         }
///     }
///
///     fn clone_request(&mut self, req: &Req) -> Option<Req> {
///         Some(req.clone())
///     }
/// }
/// ```
pub trait Policy<Req, Res, E> {
    /// The [`Future`] type returned by [`Policy::retry`].
    type Future: Future<Output = ()>;

    /// A type that is able to store request object, that can be
    /// cloned back to original request.
    type CloneableRequest;

    /// Check the policy if a certain request should be retried.
    ///
    /// This method is passed a reference to the original request, and either
    /// the [`Service::Response`] or [`Service::Error`] from the inner service.
    ///
    /// If the request should **not** be retried, return `None`.
    ///
    /// If the request *should* be retried, return `Some` future that will delay
    /// the next retry of the request. This can be used to sleep for a certain
    /// duration, to wait for some external condition to be met before retrying,
    /// or resolve right away, if the request should be retried immediately.
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
    fn retry(
        &mut self,
        req: &mut Self::CloneableRequest,
        result: Result<Res, E>,
    ) -> Outcome<Self::Future, Res, E>;

    /// Consume initial request and returns `CloneableRequest` which
    /// will be used to recreate original request objects for each retry.
    /// This is essential in cases where original request cannot be cloned,
    /// but can only be consumed.
    fn create_cloneable_request(&mut self, req: Req) -> Self::CloneableRequest;

    /// Recreates original request object for each retry.
    fn clone_request(&mut self, req: &Self::CloneableRequest) -> Req;
}

/// Outcome from [`Policy::retry`] with two choices:
/// * don retry, and just return result
/// * or retry by specifying future that might be used to control delay before next call.
#[derive(Debug)]
pub enum Outcome<Fut, Resp, Err> {
    /// Future which will allow delay retry
    Retry(Fut),
    /// Result that will be returned from middleware.
    Return(Result<Resp, Err>),
}

// Ensure `Policy` is object safe
#[cfg(test)]
fn _obj_safe(
    _: Box<dyn Policy<(), (), (), Future = futures::future::Ready<()>, CloneableRequest = ()>>,
) {
}
