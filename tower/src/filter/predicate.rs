use std::future::Future;

/// Checks a request asynchronously.
pub trait Predicate<Request> {
    /// The future returned by `check`.
    type Future: Future<Output = Result<Self::Request, Self::Error>>;
    type Request;
    type Error: Into<crate::BoxError>;

    /// Check whether the given request should be forwarded.
    ///
    /// If the future resolves with `Ok`, the request is forwarded to the inner service.
    fn check(&mut self, request: Request) -> Self::Future;
}
/// Checks a request synchronously.
pub trait SyncPredicate<Request> {
    /// The future returned by `check`.
    type Request;
    type Error: Into<crate::BoxError>;

    /// Check whether the given request should be forwarded.
    ///
    /// If the future resolves with `Ok`, the request is forwarded to the inner service.
    fn check(&mut self, request: Request) -> Result<Self::Request, Self::Error>;
}

impl<F, T, U, R, E> Predicate<T> for F
where
    F: Fn(T) -> U,
    U: Future<Output = Result<R, E>>,
    E: Into<crate::BoxError>,
{
    type Future = U;
    type Request = R;
    type Error = E;

    fn check(&mut self, request: T) -> Self::Future {
        self(request)
    }
}

impl<F, T, R, E> SyncPredicate<T> for F
where
    F: Fn(T) -> Result<R, E>,
    E: Into<crate::BoxError>,
{
    type Request = R;
    type Error = E;

    fn check(&mut self, request: T) -> Result<R, E> {
        self(request)
    }
}
