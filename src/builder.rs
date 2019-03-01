//! Builder types to compose layers and services

use futures::{Async, Future, Poll};
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;
use tower_layer::{util::Chain, Layer, LayerExt};
use tower_service::Service;

/// Configure and build a `MakeService`
///
/// `ServiceBuilder` collects layers and a `MakeService` transport
/// and produces a new `MakeService` that is wrapped by the composed
/// layer.
#[derive(Debug)]
pub struct ServiceBuilder<S, L, Target, Request>
where
    S: Service<Target>,
    S::Response: Service<Request>,
{
    maker: S,
    layer: L,
    _pd: PhantomData<(Target, Request)>,
}

/// Composed `MakeService` produced from `ServiceBuilder`
#[derive(Debug)]
pub struct ServiceBuilderMaker<S, L, Target, Request> {
    maker: S,
    layer: Arc<L>,
    _pd: PhantomData<(Target, Request)>,
}

/// Async resolve the MakeService and wrap it with the layers
#[derive(Debug)]
pub struct MakerFuture<S, L, Target, Request>
where
    S: Service<Target>,
{
    inner: S::Future,
    layer: Arc<L>,
    _pd: PhantomData<(Target, Request)>,
}

/// Errors produced from building a service stack
#[derive(Debug)]
pub enum Error<M, L> {
    /// Error produced from the MakeService
    Make(M),
    /// Error produced from building the layers
    Layer(L),
}

impl<S, Target, Request> ServiceBuilder<S, Identity, Target, Request>
where
    S: Service<Target>,
    S::Response: Service<Request>,
{
    /// Create a new `ServiceBuilder` from a `MakeService`.
    pub fn new(maker: S) -> Self {
        ServiceBuilder {
            maker,
            layer: Identity::new(),
            _pd: PhantomData,
        }
    }
}

impl<S, L, Target, Request> ServiceBuilder<S, L, Target, Request>
where
    S: Service<Target>,
    S::Response: Service<Request>,
    L: Layer<S::Response, Request>,
    Target: Clone,
{
    /// Add a layer to the `ServiceBuilder`.
    pub fn add<T>(self, layer: T) -> ServiceBuilder<S, Chain<L, T>, Target, Request>
    where
        T: Layer<L::Service, Request>,
    {
        ServiceBuilder {
            maker: self.maker,
            layer: self.layer.chain(layer),
            _pd: PhantomData,
        }
    }

    /// Create a `ServiceBuilderMaker` from the composed middleware and transport.
    pub fn build(self) -> ServiceBuilderMaker<S, L, Target, Request> {
        ServiceBuilderMaker {
            maker: self.maker,
            layer: Arc::new(self.layer),
            _pd: PhantomData,
        }
    }
}

impl<S, L, Target, Request> Service<Target> for ServiceBuilderMaker<S, L, Target, Request>
where
    S: Service<Target> + Send + 'static,
    S::Response: Service<Request> + Send + 'static,
    S::Error: Send + 'static,
    S::Future: Send + 'static,
    L: Layer<S::Response, Request> + Sync + Send + 'static,
    L::Service: Send + 'static,
    L::LayerError: fmt::Debug,
    Target: Clone,
{
    type Response = L::Service;
    type Error = Error<S::Error, L::LayerError>;
    type Future = MakerFuture<S, L, Target, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.maker.poll_ready().map_err(|e| Error::Make(e))
    }

    fn call(&mut self, target: Target) -> Self::Future {
        // let middleware = Arc::clone(&self.middleware);

        // let fut = self
        //     .maker
        //     .call(target)
        //     .and_then(move |conn| Ok(middleware.layer(conn).unwrap()));

        // // TODO(lucio): replace this with a concrete future type
        // Box::new(fut);

        let inner = self.maker.call(target);
        let layer = Arc::clone(&self.layer);
        MakerFuture {
            inner,
            layer,
            _pd: PhantomData,
        }
    }
}

impl<S, L, Target, Request> Future for MakerFuture<S, L, Target, Request>
where
    S: Service<Target>,
    S::Response: Service<Request>,
    L: Layer<S::Response, Request>,
{
    type Item = L::Service;
    type Error = Error<S::Error, L::LayerError>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let service = try_ready!(self.inner.poll().map_err(|e| Error::Make(e)));

        match self.layer.layer(service) {
            Ok(service) => Ok(Async::Ready(service)),
            Err(e) => Err(Error::Layer(e)),
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
    type LayerError = ();
    type Service = S;

    fn layer(&self, inner: S) -> Result<Self::Service, Self::LayerError> {
        Ok(inner)
    }
}
