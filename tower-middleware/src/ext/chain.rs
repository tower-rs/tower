use tower_service::Service;
use Middleware;

/// Two middlewares chained together.
///
/// This type is produced by `Middleware::chain`.
#[derive(Debug)]
pub struct Chain<Inner, Outer> {
    inner: Inner,
    outer: Outer,
}

impl<Inner, Outer> Chain<Inner, Outer> {
    /// Create a new `Chain`.
    pub fn new(inner: Inner, outer: Outer) -> Self {
        Chain { inner, outer }
    }
}

impl<S, Inner, Outer> Middleware<S> for Chain<Inner, Outer>
where
    S: Service<Outer::Request>,
    Inner: Middleware<S>,
    Outer: Middleware<Inner::Service>,
{
    type Request = Outer::Request;
    type Response = Outer::Response;
    type Error = Outer::Error;
    type Service = Outer::Service;

    fn wrap(&self, service: S) -> Self::Service {
        self.outer.wrap(self.inner.wrap(service))
    }
}
