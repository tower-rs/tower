use crate::Filter;
use tower_layer::Layer;

/// Conditionally dispatch requests to the inner service based on a predicate.
#[derive(Debug)]
pub struct FilterLayer<U> {
    predicate: U,
}

impl<U> FilterLayer<U> {
    #[allow(missing_docs)]
    pub fn new(predicate: U) -> Self {
        FilterLayer { predicate }
    }
}

impl<U: Clone, S> Layer<S> for FilterLayer<U> {
    type Service = Filter<S, U>;

    fn layer(&self, service: S) -> Self::Service {
        let predicate = self.predicate.clone();
        Filter::new(service, predicate)
    }
}
