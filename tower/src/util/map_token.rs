use std::{
    fmt,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;

/// [`Service`] returned by the [`map_token`] combinator.
///
/// [`map_future`]: crate::util::ServiceExt::map_token
#[derive(Clone)]
pub struct MapToken<S, F, G> {
    inner: S,
    f: F,
    g: G,
}

impl<S, F, G> MapToken<S, F, G> {
    /// Creates a new [`MapToken`] service.
    pub fn new(inner: S, f: F, g: G) -> Self {
        Self { inner, f, g }
    }

    /// Returns a new [`Layer`] that produces [`MapToken`] services.
    ///
    /// This is a convenience function that simply calls [`MapTokenLayer::new`].
    ///
    /// [`Layer`]: tower_layer::Layer
    pub fn layer(f: F, g: G) -> MapTokenLayer<F, G> {
        MapTokenLayer::new(f, g)
    }

    /// Get a reference to the inner service
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Get a mutable reference to the inner service
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Consume `self`, returning the inner service
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<R, S, F, G, Token> Service<R> for MapToken<S, F, G>
where
    S: Service<R>,
    F: FnMut(S::Token) -> Token,
    G: FnMut(Token) -> S::Token,
{
    type Response = S::Response;
    type Error = S::Error;
    type Token = Token;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Token, Self::Error>> {
        self.inner.poll_ready(cx).map_ok(self.f).map_err(From::from)
    }

    fn call(&mut self, token: Self::Token, req: R) -> Self::Future {
        self.inner.call((self.g)(token), req)
    }
}

impl<S, F, G> fmt::Debug for MapToken<S, F, G>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapToken")
            .field("inner", &self.inner)
            .field("f", &format_args!("{}", std::any::type_name::<F>()))
            .field("g", &format_args!("{}", std::any::type_name::<G>()))
            .finish()
    }
}

/// A [`Layer`] that produces a [`MapToken`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Clone)]
pub struct MapTokenLayer<F, G> {
    f: F,
    g: G,
}

impl<F, G> MapTokenLayer<F, G> {
    /// Creates a new [`MapTokenLayer`] layer.
    pub fn new(f: F, g: G) -> Self {
        Self { f, g }
    }
}

impl<S, F, G> Layer<S> for MapTokenLayer<F, G>
where
    F: Clone,
    G: Clone,
{
    type Service = MapToken<S, F, G>;

    fn layer(&self, inner: S) -> Self::Service {
        MapToken::new(inner, self.f.clone(), self.g.clone())
    }
}

impl<F, G> fmt::Debug for MapTokenLayer<F, G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapTokenLayer")
            .field("f", &format_args!("{}", std::any::type_name::<F>()))
            .field("g", &format_args!("{}", std::any::type_name::<G>()))
            .finish()
    }
}
