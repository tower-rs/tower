use tower_layer::Layer;
use tower_service::Service;

use error::{Error, Never};
use LoadShed;

/// A `tower-layer` to wrap services in `LoadShed` middleware.
#[derive(Debug)]
pub struct LoadShedLayer {
    _p: (),
}

impl LoadShedLayer {
    /// Creates a new layer.
    pub fn new() -> Self {
        LoadShedLayer { _p: () }
    }
}

impl<S, Req> Layer<S, Req> for LoadShedLayer
where
    S: Service<Req>,
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = Never;
    type Service = LoadShed<S>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(LoadShed::new(service))
    }
}
