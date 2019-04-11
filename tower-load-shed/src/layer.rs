use tower_layer::Layer;

use crate::LoadShed;

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

impl<S> Layer<S> for LoadShedLayer {
    type Service = LoadShed<S>;

    fn layer(&self, service: S) -> Self::Service {
        LoadShed::new(service)
    }
}
