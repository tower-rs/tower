use super::Multiplex;
use tower_layer::Layer;

/// TODO(david): docs
#[derive(Debug, Clone, Copy)]
pub struct MultiplexLayer<A, P> {
    first: A,
    picker: P,
}

impl<A, P> MultiplexLayer<A, P> {
    /// TODO(david): docs
    pub fn new(first: A, picker: P) -> Self {
        Self { picker, first }
    }
}

impl<P, A, B> Layer<B> for MultiplexLayer<A, P>
where
    P: Clone,
    A: Clone,
{
    type Service = Multiplex<A, B, P>;

    fn layer(&self, inner: B) -> Self::Service {
        Multiplex::new(self.first.clone(), inner, self.picker.clone())
    }
}
