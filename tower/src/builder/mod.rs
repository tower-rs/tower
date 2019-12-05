//! Builder types to compose layers and services

use crate::{
    buffer::BufferLayer,
    limit::{concurrency::ConcurrencyLimitLayer, rate::RateLimitLayer},
    load_shed::LoadShedLayer,
    retry::RetryLayer,
    timeout::TimeoutLayer,
};

use tower_layer::{Identity, Layer, Stack};

use std::{fmt, time::Duration};

/// Declaratively construct Service values.
///
/// `ServiceBuilder` provides a [builder-like interface][builder] for composing
/// layers to be applied to a `Service`.
///
/// # Service
///
/// A [`Service`](tower_service::Service) is a trait representing an
/// asynchronous function of a request to a response. It is similar to `async
/// fn(Request) -> Result<Response, Error>`.
///
/// A `Service` is typically bound to a single transport, such as a TCP
/// connection.  It defines how _all_ inbound or outbound requests are handled
/// by that connection.
///
///
/// [builder]: https://doc.rust-lang.org/1.0.0/style/ownership/builders.html
///
/// # Order
///
/// The order in which layers are added impacts how requests are handled. Layers
/// that are added first will be called with the request first. The argument to
/// `service` will be last to see the request.
///
/// ```
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # async fn wrap<S>(svc: S) where S: Service<(), Error = &'static str> + 'static + Send, S::Future: Send {
/// ServiceBuilder::new()
///     .buffer(100)
///     .concurrency_limit(10)
///     .service(svc)
/// # ;
/// # }
/// ```
///
/// In the above example, the buffer layer receives the request first followed
/// by `concurrency_limit`. `buffer` enables up to 100 request to be in-flight
/// **on top of** the requests that have already been forwarded to the next
/// layer. Combined with `concurrency_limit`, this allows up to 110 requests to be
/// in-flight.
///
/// ```
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # async fn wrap<S>(svc: S) where S: Service<(), Error = &'static str> + 'static + Send, S::Future: Send {
/// ServiceBuilder::new()
///     .concurrency_limit(10)
///     .buffer(100)
///     .service(svc)
/// # ;
/// # }
/// ```
///
/// The above example is similar, but the order of layers is reversed. Now,
/// `concurrency_limit` applies first and only allows 10 requests to be in-flight
/// total.
///
/// # Examples
///
/// A `Service` stack with a single layer:
///
/// ```
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # use tower_limit::concurrency::ConcurrencyLimitLayer;
/// # async fn wrap<S>(svc: S) where S: Service<(), Error = &'static str> + 'static + Send, S::Future: Send {
/// ServiceBuilder::new()
///     .concurrency_limit(5)
///     .service(svc);
/// # ;
/// # }
/// ```
///
/// A `Service` stack with _multiple_ layers that contain rate limiting,
/// in-flight request limits, and a channel-backed, clonable `Service`:
///
/// ```
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # use std::time::Duration;
/// # async fn wrap<S>(svc: S) where S: Service<(), Error = &'static str> + 'static + Send, S::Future: Send {
/// ServiceBuilder::new()
///     .buffer(5)
///     .concurrency_limit(5)
///     .rate_limit(5, Duration::from_secs(1))
///     .service(svc);
/// # ;
/// # }
/// ```
#[derive(Clone)]
pub struct ServiceBuilder<L> {
    layer: L,
}

impl ServiceBuilder<Identity> {
    /// Create a new `ServiceBuilder`.
    pub fn new() -> Self {
        ServiceBuilder {
            layer: Identity::new(),
        }
    }
}

impl<L> ServiceBuilder<L> {
    /// Add a new layer `T` into the `ServiceBuilder`.
    pub fn layer<T>(self, layer: T) -> ServiceBuilder<Stack<T, L>> {
        ServiceBuilder {
            layer: Stack::new(layer, self.layer),
        }
    }

    /// Buffer requests when when the next layer is out of capacity.
    pub fn buffer<Request>(self, bound: usize) -> ServiceBuilder<Stack<BufferLayer<Request>, L>> {
        self.layer(BufferLayer::new(bound))
    }

    /// Limit the max number of in-flight requests.
    ///
    /// A request is in-flight from the time the request is received until the
    /// response future completes. This includes the time spent in the next
    /// layers.
    pub fn concurrency_limit(self, max: usize) -> ServiceBuilder<Stack<ConcurrencyLimitLayer, L>> {
        self.layer(ConcurrencyLimitLayer::new(max))
    }

    /// Drop requests when the next layer is unable to respond to requests.
    ///
    /// Usually, when a layer or service does not have capacity to process a
    /// request (i.e., `poll_ready` returns `NotReady`), the caller waits until
    /// capacity becomes available.
    ///
    /// `load_shed` immediately responds with an error when the next layer is
    /// out of capacity.
    pub fn load_shed(self) -> ServiceBuilder<Stack<LoadShedLayer, L>> {
        self.layer(LoadShedLayer::new())
    }

    /// Limit requests to at most `num` per the given duration
    pub fn rate_limit(self, num: u64, per: Duration) -> ServiceBuilder<Stack<RateLimitLayer, L>> {
        self.layer(RateLimitLayer::new(num, per))
    }

    /// Retry failed requests.
    ///
    /// `policy` must implement [`Policy`].
    ///
    /// [`Policy`]: ../retry/trait.Policy.html
    pub fn retry<P>(self, policy: P) -> ServiceBuilder<Stack<RetryLayer<P>, L>> {
        self.layer(RetryLayer::new(policy))
    }

    /// Fail requests that take longer than `timeout`.
    ///
    /// If the next layer takes more than `timeout` to respond to a request,
    /// processing is terminated and an error is returned.
    pub fn timeout(self, timeout: Duration) -> ServiceBuilder<Stack<TimeoutLayer, L>> {
        self.layer(TimeoutLayer::new(timeout))
    }

    /// Obtains the underlying `Layer` implementation.
    pub fn into_inner(self) -> L {
        self.layer
    }

    /// Wrap the service `S` with the layers.
    pub fn service<S>(self, service: S) -> L::Service
    where
        L: Layer<S>,
    {
        self.layer.layer(service)
    }
}

impl<L: fmt::Debug> fmt::Debug for ServiceBuilder<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ServiceBuilder").field(&self.layer).finish()
    }
}
