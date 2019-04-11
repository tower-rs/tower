use crate::Service;
use futures::{try_ready, Async, Future, Poll};
use std::{marker::PhantomData, sync::Arc};
use tower_layer::Layer;
use tower_util::MakeService;

/// Composed `MakeService` produced from `ServiceBuilder`
#[derive(Debug)]
pub struct LayeredMakeService<S, L, Request> {
    maker: S,
    layer: Arc<L>,
    _pd: PhantomData<Request>,
}

/// Async resolve the MakeService and wrap it with the layers
#[derive(Debug)]
pub struct ServiceFuture<S, L, Target, Request>
where
    S: MakeService<Target, Request>,
{
    inner: S::Future,
    layer: Arc<L>,
}

impl<S, L, Request> LayeredMakeService<S, L, Request> {
    pub(crate) fn new(maker: S, layer: L) -> Self {
        LayeredMakeService {
            maker,
            layer: Arc::new(layer),
            _pd: PhantomData,
        }
    }
}

impl<S, L, Target, Request> Service<Target> for LayeredMakeService<S, L, Request>
where
    S: MakeService<Target, Request>,
    L: Layer<S::Service> + Sync + Send + 'static,
    Target: Clone,
{
    type Response = L::Service;
    type Error = S::MakeError;
    type Future = ServiceFuture<S, L, Target, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.maker.poll_ready()
    }

    fn call(&mut self, target: Target) -> Self::Future {
        let inner = self.maker.make_service(target);
        let layer = Arc::clone(&self.layer);

        ServiceFuture { inner, layer }
    }
}

impl<S, L, Target, Request> Future for ServiceFuture<S, L, Target, Request>
where
    S: MakeService<Target, Request>,
    L: Layer<S::Service>,
{
    type Item = L::Service;
    type Error = S::MakeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let service = try_ready!(self.inner.poll());
        let service = self.layer.layer(service);
        Ok(Async::Ready(service))
    }
}
