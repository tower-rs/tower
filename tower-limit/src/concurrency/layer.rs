use super::{LimitConcurrency, Error};
use super::never::Never;
use tower_layer::Layer;
use tower_service::Service;

#[derive(Debug, Clone)]
pub struct LimitConcurrencyLayer {
    max: usize,
}

impl LimitConcurrencyLayer {
    pub fn new(max: usize) -> Self {
        LimitConcurrencyLayer { max }
    }
}

impl<S, Request> Layer<S, Request> for LimitConcurrencyLayer
where
    S: Service<Request>,
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = Never;
    type Service = LimitConcurrency<S>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(LimitConcurrency::new(service, self.max))
    }
}
