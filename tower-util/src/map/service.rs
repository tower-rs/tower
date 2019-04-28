use futures::Poll;

use tower_service::Service;

/// Map the reqeust from type A to type B
#[derive(Debug)]
pub struct MapRequest<S, M> {
  /// the inner service
  inner: S,
  /// the function to convert from the upstream request type
  /// to the inner request type
  m: M,
}

impl<S, M> MapRequest<S, M> {
  /// Create a new rate limiter
  pub fn new(m: M, inner: S) -> Self {
    MapRequest { m, inner }
  }
}

impl<S, M, O, Request> Service<Request> for MapRequest<S, M>
where
  // Func(Request) -> Future<S::Request, ?>
  M: FnMut(Request) -> O,
  S: Service<O>,
{
  type Response = S::Response;
  type Error = S::Error;
  type Future = S::Future;

  fn poll_ready(&mut self) -> Poll<(), Self::Error> {
    self.inner.poll_ready().map_err(Into::into)
  }

  fn call(&mut self, request: Request) -> Self::Future {
    // convert the inbond request to the downstream
    // request. Then handle that the conversion
    // returns a future as well.
    let transformed = (self.m)(request);

    self.inner.call(transformed)
  }
}
