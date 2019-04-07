use crate::error::{self, Error};
use tower_layer::Layer;
use tower_service::Service;
use crate::{Filter, Predicate};

pub struct FilterLayer<U> {
    predicate: U,
}

impl<U> FilterLayer<U> {
    pub fn new(predicate: U) -> Self {
        FilterLayer { predicate }
    }
}

impl<U, S, Request> Layer<S, Request> for FilterLayer<U>
where
    U: Predicate<Request> + Clone,
    S: Service<Request> + Clone,
    S::Error: Into<error::Source>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = error::never::Never;
    type Service = Filter<S, U>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        let predicate = self.predicate.clone();
        Ok(Filter::new(service, predicate))
    }
}
