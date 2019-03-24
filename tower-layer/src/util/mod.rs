//! Types and utilities for working with `Layer`.

use Layer;

mod chain;
mod identity;

pub use self::chain::{Chain, ChainError};
pub use self::identity::Identity;

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
