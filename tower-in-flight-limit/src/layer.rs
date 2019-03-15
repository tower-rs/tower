use tower_layer::{util::Never, Layer};
use tower_service::Service;
use {Error, InFlightLimit};

#[derive(Debug, Clone)]
pub struct InFlightLimitLayer {
    max: usize,
}

impl InFlightLimitLayer {
    pub fn new(max: usize) -> Self {
        InFlightLimitLayer { max }
    }
}

impl<S, Request> Layer<S, Request> for InFlightLimitLayer
where
    S: Service<Request>,
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = Never;
    type Service = InFlightLimit<S>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(InFlightLimit::new(service, self.max))
    }
}
