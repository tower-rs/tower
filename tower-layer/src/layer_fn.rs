use super::Layer;
use std::fmt;

/// Returns a new `LayerFn` with the given closure.
pub fn layer_fn<T>(f: T) -> LayerFn<T> {
    LayerFn { f }
}

/// A `Layer` implemented by a closure.
#[derive(Clone, Copy)]
pub struct LayerFn<F> {
    f: F,
}

impl<F, S, Out> Layer<S> for LayerFn<F>
where
    F: Fn(S) -> Out,
{
    type Service = Out;

    fn layer(&self, inner: S) -> Self::Service {
        (self.f)(inner)
    }
}

impl<F> fmt::Debug for LayerFn<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LayerFn")
            .field("f", &format_args!("<{}>", std::any::type_name::<F>()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    #[test]
    fn layer_fn_has_useful_debug_impl() {
        struct WrappedService<S> {
            inner: S,
        }
        let layer = layer_fn(|svc| WrappedService { inner: svc });
        let _svc = layer.layer("foo");

        assert_eq!(
            "LayerFn { f: <tower_layer::layer_fn::tests::layer_fn_has_useful_debug_impl::{{closure}}> }".to_string(),
            format!("{:?}", layer),
        );
    }
}
