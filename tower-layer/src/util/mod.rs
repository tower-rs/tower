//! Types and utilities for working with `Layer`.

use Layer;

mod chain;

pub use self::chain::{Chain, ChainError};

/// An extension trait for `Layer`'s that provides a variety of convenient
/// adapters.
pub trait LayerExt<S, Request>: Layer<S, Request> {
    /// Return a new `Layer` instance that applies both `self` and
    /// `middleware` to services being wrapped.
    ///
    /// This defines a middleware stack.
    fn chain<T>(self, middleware: T) -> Chain<Self, T>
    where
        T: Layer<S, Request>,
        Self: Sized,
    {
        Chain::new(self, middleware)
    }
}

impl<T, S, Request> LayerExt<S, Request> for T where T: Layer<S, Request> {}

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
