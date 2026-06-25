//! A [`Layer`] that is enabled or disabled by an [`Option`].
//!
//! See [`OptionLayer`] and [`option_layer`] for more details.
//!
//! [`option_layer`]: crate::util::option_layer

use crate::BoxError;
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;

/// A [`Layer`] that optionally applies an inner layer `L`.
///
/// This is the layer produced by [`option_layer`]. When the inner layer is
/// present, the resulting service is the layered service; when it is absent,
/// the resulting service is the unmodified service.
///
/// Unlike branching with [`Either`] directly, [`OptionLayer`] unifies the error
/// types of the two branches to [`BoxError`]. This means the optional layer is
/// allowed to change the error type (as [`TimeoutLayer`] does, for example)
/// without the two branches needing to share an error type.
///
/// [`option_layer`]: crate::util::option_layer
/// [`Either`]: crate::util::Either
/// [`BoxError`]: crate::BoxError
/// [`TimeoutLayer`]: crate::timeout::TimeoutLayer
#[derive(Clone, Copy, Debug)]
pub struct OptionLayer<L> {
    layer: Option<L>,
}

impl<L> OptionLayer<L> {
    /// Create a new [`OptionLayer`] wrapping the given optional layer.
    pub const fn new(layer: Option<L>) -> Self {
        OptionLayer { layer }
    }
}

impl<L> From<Option<L>> for OptionLayer<L> {
    fn from(layer: Option<L>) -> Self {
        OptionLayer::new(layer)
    }
}

impl<S, L> Layer<S> for OptionLayer<L>
where
    L: Layer<S>,
{
    type Service = OptionService<L::Service, S>;

    fn layer(&self, inner: S) -> Self::Service {
        match &self.layer {
            Some(layer) => OptionService::Some(layer.layer(inner)),
            None => OptionService::None(inner),
        }
    }
}

/// The [`Service`] produced by [`OptionLayer`].
///
/// Its error type is [`BoxError`], erasing any difference between the layered
/// and unlayered branches' error types.
///
/// [`BoxError`]: crate::BoxError
#[derive(Clone, Copy, Debug)]
pub enum OptionService<A, B> {
    /// The inner layer was present; the layered service.
    Some(A),
    /// The inner layer was absent; the unmodified service.
    None(B),
}

impl<A, B, Request> Service<Request> for OptionService<A, B>
where
    A: Service<Request>,
    A::Error: Into<BoxError>,
    B: Service<Request, Response = A::Response>,
    B::Error: Into<BoxError>,
{
    type Response = A::Response;
    type Error = BoxError;
    type Future = ResponseFuture<A::Future, B::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self {
            OptionService::Some(service) => service.poll_ready(cx).map_err(Into::into),
            OptionService::None(service) => service.poll_ready(cx).map_err(Into::into),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match self {
            OptionService::Some(service) => ResponseFuture {
                kind: Kind::Some {
                    inner: service.call(request),
                },
            },
            OptionService::None(service) => ResponseFuture {
                kind: Kind::None {
                    inner: service.call(request),
                },
            },
        }
    }
}

pin_project! {
    /// Response future for [`OptionService`].
    pub struct ResponseFuture<A, B> {
        #[pin]
        kind: Kind<A, B>,
    }
}

pin_project! {
    #[project = KindProj]
    enum Kind<A, B> {
        Some { #[pin] inner: A },
        None { #[pin] inner: B },
    }
}

impl<A, B, T, AE, BE> Future for ResponseFuture<A, B>
where
    A: Future<Output = Result<T, AE>>,
    AE: Into<BoxError>,
    B: Future<Output = Result<T, BE>>,
    BE: Into<BoxError>,
{
    type Output = Result<T, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().kind.project() {
            KindProj::Some { inner } => inner.poll(cx).map_err(Into::into),
            KindProj::None { inner } => inner.poll(cx).map_err(Into::into),
        }
    }
}
