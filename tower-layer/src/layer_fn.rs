use crate::Layer;

/// Makes an anonymous `Layer` from a closure.
pub fn layer_fn<S, T, F>(f: F) -> impl Layer<S, Service = T> + Clone
where
    F: Fn(S) -> T,
    F: Clone,
{
    LayerFn(f)
}

#[derive(Clone)]
struct LayerFn<F>(F);

impl<F, S, T> Layer<S> for LayerFn<F>
where
    F: Fn(S) -> T,
{
    type Service = T;

    fn layer(&self, inner: S) -> Self::Service {
        (self.0)(inner)
    }
}
