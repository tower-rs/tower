use crate::Filter;
use tower_layer::Layer;

pub struct FilterLayer<U> {
    predicate: U,
}

impl<U> FilterLayer<U> {
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
