use crate::Map;
use std::marker::PhantomData;
use tower_layer::Layer;
use tower_service::Service;

/// Map requests to different types.
#[derive(Debug)]
pub struct MapLayer<F, R1, R2> {
    func: F,
    _r1: PhantomData<R1>,
    _r2: PhantomData<R2>,
}

impl<F, R1, R2> MapLayer<F, R1, R2> {
    /// Create a new `MapLayer` from a closure.
    pub fn new(func: F) -> Self {
        MapLayer {
            func,
            _r1: PhantomData,
            _r2: PhantomData,
        }
    }
}

impl<F, R1, R2, S> Layer<S> for MapLayer<F, R1, R2>
where
    S: Service<R2>,
    F: Fn(R1) -> R2 + Clone,
{
    type Service = Map<F, S>;

    fn layer(&self, service: S) -> Self::Service {
        Map::new(self.func.clone(), service)
    }
}
