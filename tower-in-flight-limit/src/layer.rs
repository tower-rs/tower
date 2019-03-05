use Error;
use InFlightLimit;
use tower_layer::Layer;
use tower_service::Service;

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
{
    type Response = S::Response;
    type Error = Error<S::Error>;
    type LayerError = ();
    type Service = InFlightLimit<S>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(InFlightLimit::new(service, self.max))
    }
}
