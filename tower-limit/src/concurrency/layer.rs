use super::{never::Never, ConcurrencyLimit, Error};
use tower_layer::Layer;
use tower_service::Service;

#[derive(Debug, Clone)]
pub struct ConcurrencyLimitLayer {
    max: usize,
}

impl ConcurrencyLimitLayer {
    pub fn new(max: usize) -> Self {
        ConcurrencyLimitLayer { max }
    }
}

impl<S, Request> Layer<S, Request> for ConcurrencyLimitLayer
where
    S: Service<Request>,
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = Never;
    type Service = ConcurrencyLimit<S>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(ConcurrencyLimit::new(service, self.max))
    }
}
