//! Builder types to compose layers and services

use futures::{Async, Future, Poll};
use std::marker::PhantomData;
use std::sync::Arc;
use tower_layer::{
    util::{Chain, Never},
    Layer, LayerExt,
};
use tower_service::Service;
use tower_util::MakeService;

/// Configure and build a `MakeService`
///
/// `ServiceBuilder` collects layers and a `MakeService` transport
/// and produces a new `MakeService` that is wrapped by the composed
/// layer.
#[derive(Debug)]
pub struct ServiceBuilder<S, L> {
    maker: S,
    layer: L,
}

/// Composed `MakeService` produced from `ServiceBuilder`
#[derive(Debug)]
pub struct ServiceBuilderMaker<S, L, Request> {
    maker: S,
    layer: Arc<L>,
    _pd: PhantomData<Request>,
}

/// Async resolve the MakeService and wrap it with the layers
#[derive(Debug)]
pub struct MakerFuture<S, L, Target, Request>
where
    S: MakeService<Target, Request>,
{
    inner: S::Future,
    layer: Arc<L>,
}

type Error = Box<::std::error::Error + Send + Sync>;

impl<S> ServiceBuilder<S, Identity> {
    /// Create a new `ServiceBuilder` from a `MakeService`.
    pub fn new(maker: S) -> Self {
        ServiceBuilder {
            maker,
            layer: Identity::new(),
        }
    }
}

impl<S, L> ServiceBuilder<S, L> {
    /// Add a layer `T` to the `ServiceBuilder`.
    pub fn add<T, Target, Request>(self, layer: T) -> ServiceBuilder<S, Chain<L, T>>
    where
        S: MakeService<Target, Request>,
        T: Layer<L::Service, Request>,
        L: Layer<S::Service, Request>,
        Target: Clone,
    {
        ServiceBuilder {
            maker: self.maker,
            layer: self.layer.chain(layer),
        }
    }

    /// Create a `ServiceBuilderMaker` from the composed middleware and transport.
    pub fn build<Request>(self) -> ServiceBuilderMaker<S, L, Request> {
        ServiceBuilderMaker {
            maker: self.maker,
            layer: Arc::new(self.layer),
            _pd: PhantomData,
        }
    }
}

impl<S, L, Target, Request> Service<Target> for ServiceBuilderMaker<S, L, Request>
where
    S: MakeService<Target, Request>,
    S::MakeError: Into<Error>,
    L: Layer<S::Service, Request> + Sync + Send + 'static,
    L::LayerError: Into<Error>,
    Target: Clone,
{
    type Response = L::Service;
    type Error = Error;
    type Future = MakerFuture<S, L, Target, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.maker.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, target: Target) -> Self::Future {
        let inner = self.maker.make_service(target);
        let layer = Arc::clone(&self.layer);

        MakerFuture { inner, layer }
    }
}

impl<S, L, Target, Request> Future for MakerFuture<S, L, Target, Request>
where
    S: MakeService<Target, Request>,
    S::MakeError: Into<Error>,
    L: Layer<S::Service, Request>,
    L::LayerError: Into<Error>,
{
    type Item = L::Service;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let service = try_ready!(self.inner.poll().map_err(Into::into));

        match self.layer.layer(service) {
            Ok(service) => Ok(Async::Ready(service)),
            Err(e) => Err(e.into()),
        }
    }
}

/// A no-op middleware.
///
/// When wrapping a `Service`, the `Identity` layer returns the provided
/// service without modifying it.
#[derive(Debug, Default, Clone)]
pub struct Identity {
    _p: (),
}

impl Identity {
    /// Create a new `Identity` value
    pub fn new() -> Identity {
        Identity { _p: () }
    }
}

/// Decorates a `Service`, transforming either the request or the response.
impl<S, Request> Layer<S, Request> for Identity
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type LayerError = Never;
    type Service = S;

    fn layer(&self, inner: S) -> Result<Self::Service, Self::LayerError> {
        Ok(inner)
    }
}
