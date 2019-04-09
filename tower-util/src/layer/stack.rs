use tower_layer::Layer;
use tower_service::Service;

/// Two middlewares chained together.
///
/// This type is produced by `Layer::chain`.
#[derive(Debug)]
pub struct Stack<Inner, Outer> {
    inner: Inner,
    outer: Outer,
}

type Error = Box<dyn std::error::Error + Send + Sync>;

impl<Inner, Outer> Stack<Inner, Outer> {
    /// Create a new `Stack`.
    pub fn new(inner: Inner, outer: Outer) -> Self {
        Stack { inner, outer }
    }
}

impl<S, Request, Inner, Outer> Layer<S, Request> for Stack<Inner, Outer>
where
    S: Service<Request>,
    Inner: Layer<S, Request>,
    Inner::LayerError: Into<Error>,
    Outer: Layer<Inner::Service, Request>,
    Outer::LayerError: Into<Error>,
{
    type Response = Outer::Response;
    type Error = Outer::Error;
    type LayerError = Error;
    type Service = Outer::Service;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        let inner = self.inner.layer(service).map_err(Into::into)?;

        self.outer.layer(inner).map_err(Into::into)
    }
}
