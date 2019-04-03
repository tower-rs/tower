//! A collection of `Layer` based tower services

pub use tower_layer::Layer;

pub use buffer::BufferLayer;
pub use filter::FilterLayer;
pub use in_flight_limit::InFlightLimitLayer;
pub use load_shed::LoadShedLayer;
pub use rate_limit::RateLimitLayer;
pub use retry::RetryLayer;
pub use timeout::TimeoutLayer;

pub mod util {
    pub use tower_util::layer::Chain;
    pub use tower_util::layer::Identity;
}

/// An extension trait for `Layer`'s that provides a variety of convenient
/// adapters.
pub trait LayerExt<S, Request>: Layer<S, Request> {
    /// Return a new `Layer` instance that applies both `self` and
    /// `middleware` to services being wrapped.
    ///
    /// This defines a middleware stack.
    fn chain<T>(self, middleware: T) -> util::Chain<Self, T>
    where
        T: Layer<Self::Service, Request>,
        Self: Sized,
    {
        util::Chain::new(self, middleware)
    }
}

impl<T, S, Request> LayerExt<S, Request> for T where T: Layer<S, Request> {}
