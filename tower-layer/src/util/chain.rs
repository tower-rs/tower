use tower_service::Service;
use Layer;

/// Two middlewares chained together.
///
/// This type is produced by `Layer::chain`.
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

impl<S, Request, Inner, Outer> Layer<S, Request> for Chain<Inner, Outer>
where
    S: Service<Request>,
    Inner: Layer<S, Request>,
    Outer: Layer<Inner::Service, Request>,
{
    type Response = Outer::Response;
    type Error = Outer::Error;
    type Service = Outer::Service;

    fn layer(&self, service: S) -> Self::Service {
        self.outer.wrap(self.inner.wrap(service))
    }
}
