use super::MapRequest;
use tower_layer::Layer;

/// Map inbound requests to a new type
#[derive(Debug, Clone)]
pub struct MapRequestLayer<M> {
  m: M,
}

impl<M> MapRequestLayer<M> {
  /// Create a new `MapRequestLayer` from a closure
  pub fn new(m: M) -> Self {
    MapRequestLayer { m }
  }
}

impl<S, M> Layer<S> for MapRequestLayer<M>
where
  M: Clone,
{
  type Service = MapRequest<S, M>;

  fn layer(&self, service: S) -> Self::Service {
    MapRequest::new(self.m.clone(), service)
  }
}
