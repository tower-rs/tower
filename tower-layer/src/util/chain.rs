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

/// Error's produced when chaining two layers together
#[derive(Debug)]
pub enum ChainError<I, O> {
    /// Error produced from the inner layer call
    Inner(I),
    /// Error produced from the outer layer call
    Outer(O),
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
    type LayerError = ChainError<Inner::LayerError, Outer::LayerError>;
    type Service = Outer::Service;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        let inner = self
            .inner
            .layer(service)
            .map_err(|e| ChainError::Inner(e))?;

        self.outer.layer(inner).map_err(|e| ChainError::Outer(e))
    }
}
