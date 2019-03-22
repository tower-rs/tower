//! Builder types to compose layers and services

use futures::{Async, Future, Poll};
use std::marker::PhantomData;
use std::sync::Arc;
use tower_layer::{util::Chain, Layer, LayerExt};
use tower_service::Service;
use tower_service_util::MakeService;

use never::Never;

/// Configure and build a `MakeService`
///
/// `ServiceBuilder` collects layers and a `MakeService` transport
/// and produces a new `MakeService` that is wrapped by the composed
/// layer.
#[derive(Debug)]
pub struct ServiceBuilder<L, S, Request> {
    layer: L,
    _pd: PhantomData<(S, Request)>,
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

impl<S, Request> ServiceBuilder<Identity, S, Request> {
    /// Create a new `ServiceBuilder` from a `MakeService`.
    pub fn new() -> Self {
        ServiceBuilder {
            layer: Identity::new(),
            _pd: PhantomData,
        }
    }
}

impl<L, S, Request> ServiceBuilder<L, S, Request> {
    /// Chain a layer `T` to the `ServiceBuilder`.
    pub fn chain<T>(self, layer: T) -> ServiceBuilder<Chain<L, T>, S, Request>
    where
        L: Layer<S, Request>,
        T: Layer<L::Service, Request>,
    {
        ServiceBuilder {
            layer: self.layer.chain(layer),
            _pd: PhantomData,
        }
    }

    /// Create a `ServiceBuilderMaker` from the composed middleware and transport.
    pub fn build_maker<M, Target>(self, maker: M) -> ServiceBuilderMaker<M, L, Request>
    where
        M: MakeService<Target, Request, Service = S, Response = S::Response, Error = S::Error>,
        S: Service<Request>,
    {
        ServiceBuilderMaker {
            maker,
            layer: Arc::new(self.layer),
            _pd: PhantomData,
        }
    }

    /// Wrap the service `S` with the layers.
    pub fn build_svc(self, service: S) -> Result<L::Service, L::LayerError>
    where
        L: Layer<S, Request>,
        S: Service<Request>,
    {
        self.layer.layer(service)
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
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = S::Error;
    type LayerError = Never;
    type Service = S;

    fn layer(&self, inner: S) -> Result<Self::Service, Self::LayerError> {
        Ok(inner)
    }
}
