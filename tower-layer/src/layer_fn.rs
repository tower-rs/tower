use super::Layer;

/// Returns a new `impl Layer` with the given closure.
pub fn layer_fn<F, S, T>(f: F) -> impl Layer<S, Service = T> + Clone
where
    F: Fn(S) -> T + Clone,
{
    LayerFn { f }
}

#[derive(Clone, Copy, Debug)]
struct LayerFn<F> {
    f: F,
}

impl<F, S, T> Layer<S> for LayerFn<F>
where
    F: Fn(S) -> T,
{
    type Service = T;

    fn layer(&self, inner: S) -> Self::Service {
        (self.f)(inner)
    }
}
