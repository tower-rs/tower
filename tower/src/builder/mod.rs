//! Builder types to compose layers and services

mod service;

pub use self::service::{LayeredMakeService, ServiceFuture};

use buffer::BufferLayer;
use filter::FilterLayer;
use limit::concurrency::LimitConcurrencyLayer;
use limit::rate::LimitRateLayer;
use load_shed::LoadShedLayer;
use retry::RetryLayer;
use timeout::TimeoutLayer;

use tower_layer::Layer;
use tower_service::Service;
use tower_util::layer::{Chain, Identity};
use tower_util::MakeService;

use std::time::Duration;

pub(super) type Error = Box<::std::error::Error + Send + Sync>;

/// Declaratively construct Service values.
///
/// `ServiceBuilder` provides a [builder-like interface][builder] for composing
/// layers and a connection, where the latter is modeled by a `MakeService`. The
/// builder produces either a new `Service` or `MakeService`,
/// depending on whether `service` or `make_service` is called.
///
/// # Services and MakeServices
///
/// - A [`Service`](tower_service::Service) is a trait representing an
///   asynchronous function of a request to a response. It is similar to `async
///   fn(Request) -> Result<Response, Error>`.
///
/// - A [`MakeService`](tower_util::MakeService) is a trait creating specific
///   instances of a `Service`
///
/// # Service
///
/// A `Service` is typically bound to a single transport, such as a TCP
/// connection.  It defines how _all_ inbound or outbound requests are handled
/// by that connection.
///
/// # MakeService
///
/// Since a `Service` is bound to a single connection, a `MakeService` allows
/// for the creation of _new_ `Service`s that'll be bound to _different_
/// different connections.  This is useful for servers, as they require the
/// ability to accept new connections.
///
/// Resources that need to be shared by all `Service`s can be put into a
/// `MakeService`, and then passed to individual `Service`s when `make_service`
/// is called.
///
/// [builder]: https://doc.rust-lang.org/1.0.0/style/ownership/builders.html
///
/// # Order
///
/// The order in which layers are added impacts how requests are handled. Layers
/// that are added first will be called with the request first. The argument to
/// `service` or `make_service` will be last to see the request.
///
/// ```
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # fn dox<T>(my_service: T)
/// # where T: Service<()> + Send + 'static,
/// # T::Future: Send,
/// # T::Error: Into<Box<::std::error::Error + Send + Sync>>,
/// # {
/// ServiceBuilder::new()
///     .buffer(100)
///     .limit_concurrency(10)
///     .service(my_service)
/// # ;
/// # }
/// ```
///
/// In the above example, the buffer layer receives the request first followed
/// by `limit_concurrency`. `buffer` enables up to 100 request to be in-flight
/// **on top of** the requests that have already been forwarded to the next
/// layer. Combined with `limit_concurrency`, this allows up to 110 requests to be
/// in-flight.
///
/// ```
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # fn dox<T>(my_service: T)
/// # where T: Service<()> + Send + 'static,
/// # T::Future: Send,
/// # T::Error: Into<Box<::std::error::Error + Send + Sync>>,
/// # {
/// ServiceBuilder::new()
///     .limit_concurrency(10)
///     .buffer(100)
///     .service(my_service)
/// # ;
/// # }
/// ```
///
/// The above example is similar, but the order of layers is reversed. Now,
/// `limit_concurrency` applies first and only allows 10 requests to be in-flight
/// total.
///
/// # Examples
///
/// A MakeService stack with a single layer:
///
/// ```rust
/// # extern crate tower;
/// # extern crate tower_limit;
/// # extern crate futures;
/// # extern crate void;
/// # use void::Void;
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # use tower_limit::concurrency::LimitConcurrencyLayer;
/// # use futures::{Poll, future::{self, FutureResult}};
/// # #[derive(Debug)]
/// # struct MyMakeService;
/// # impl Service<()> for MyMakeService {
/// #    type Response = MyService;
/// #    type Error = Void;
/// #    type Future = FutureResult<Self::Response, Self::Error>;
/// #    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
/// #        Ok(().into())
/// #    }
/// #    fn call(&mut self, _: ()) -> Self::Future {
/// #        future::ok(MyService)
/// #    }
/// # }
/// # #[derive(Debug)]
/// # struct MyService;
/// # impl Service<()> for MyService {
/// #    type Response = ();
/// #    type Error = Void;
/// #    type Future = FutureResult<Self::Response, Self::Error>;
/// #    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
/// #        Ok(().into())
/// #    }
/// #    fn call(&mut self, _: ()) -> Self::Future {
/// #        future::ok(())
/// #    }
/// # }
/// ServiceBuilder::new()
///     .limit_concurrency(5)
///     .make_service(MyMakeService);
/// ```
///
/// A `Service` stack with a single layer:
///
/// ```
/// # extern crate tower;
/// # extern crate tower_limit;
/// # extern crate futures;
/// # extern crate void;
/// # use void::Void;
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # use tower_limit::concurrency::LimitConcurrencyLayer;
/// # use futures::{Poll, future::{self, FutureResult}};
/// # #[derive(Debug)]
/// # struct MyService;
/// # impl Service<()> for MyService {
/// #    type Response = ();
/// #    type Error = Void;
/// #    type Future = FutureResult<Self::Response, Self::Error>;
/// #    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
/// #        Ok(().into())
/// #    }
/// #    fn call(&mut self, _: ()) -> Self::Future {
/// #        future::ok(())
/// #    }
/// # }
/// ServiceBuilder::new()
///     .limit_concurrency(5)
///     .service(MyService);
/// ```
///
/// A `Service` stack with _multiple_ layers that contain rate limiting,
/// in-flight request limits, and a channel-backed, clonable `Service`:
///
/// ```
/// # extern crate tower;
/// # extern crate futures;
/// # extern crate void;
/// # use void::Void;
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # use std::time::Duration;
/// # use futures::{Poll, future::{self, FutureResult}};
/// # #[derive(Debug)]
/// # struct MyService;
/// # impl Service<()> for MyService {
/// #    type Response = ();
/// #    type Error = Void;
/// #    type Future = FutureResult<Self::Response, Self::Error>;
/// #    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
/// #        Ok(().into())
/// #    }
/// #    fn call(&mut self, _: ()) -> Self::Future {
/// #        future::ok(())
/// #    }
/// # }
/// ServiceBuilder::new()
///     .buffer(5)
///     .limit_concurrency(5)
///     .limit_rate(5, Duration::from_secs(1))
///     .service(MyService);
/// ```
#[derive(Debug)]
pub struct ServiceBuilder<L> {
    layer: L,
}

impl ServiceBuilder<Identity> {
    /// Create a new `ServiceBuilder` from a `MakeService`.
    pub fn new() -> Self {
        ServiceBuilder {
            layer: Identity::new(),
        }
    }
}

impl<L> ServiceBuilder<L> {
    /// Layer a new layer `T` onto the `ServiceBuilder`.
    pub fn layer<T>(self, layer: T) -> ServiceBuilder<Chain<T, L>> {
        ServiceBuilder {
            layer: Chain::new(layer, self.layer),
        }
    }

    /// Buffer requests when when the next layer is out of capacity.
    pub fn buffer(self, bound: usize) -> ServiceBuilder<Chain<BufferLayer, L>> {
        self.layer(BufferLayer::new(bound))
    }

    /// Filter requests using the given `predicate`.
    ///
    /// The `predicate` checks the request and determines if it should be
    /// forwarded to the next layer or immediately respond with an error.
    ///
    /// `predicate` must implement [`Predicate`]
    ///
    /// [`Predicate`]: ../filter/trait.Predicate.html
    pub fn filter<U>(self, predicate: U) -> ServiceBuilder<Chain<FilterLayer<U>, L>> {
        self.layer(FilterLayer::new(predicate))
    }

    /// Limit the max number of in-flight requests.
    ///
    /// A request is in-flight from the time the request is received until the
    /// response future completes. This includes the time spent in the next
    /// layers.
    pub fn limit_concurrency(self, max: usize) -> ServiceBuilder<Chain<LimitConcurrencyLayer, L>> {
        self.layer(LimitConcurrencyLayer::new(max))
    }

    /// Drop requests when the next layer is unable to respond to requests.
    ///
    /// Usually, when a layer or service does not have capacity to process a
    /// request (i.e., `poll_ready` returns `NotReady`), the caller waits until
    /// capacity becomes available.
    ///
    /// `load_shed` immediately responds with an error when the next layer is
    /// out of capacity.
    pub fn load_shed(self) -> ServiceBuilder<Chain<LoadShedLayer, L>> {
        self.layer(LoadShedLayer::new())
    }

    /// Limit requests to at most `num` per the given duration
    pub fn limit_rate(self, num: u64, per: Duration) -> ServiceBuilder<Chain<LimitRateLayer, L>> {
        self.layer(LimitRateLayer::new(num, per))
    }

    /// Retry failed requests.
    ///
    /// `policy` must implement [`Policy`].
    ///
    /// [`Policy`]: ../retry/trait.Policy.html
    pub fn retry<P>(self, policy: P) -> ServiceBuilder<Chain<RetryLayer<P>, L>> {
        self.layer(RetryLayer::new(policy))
    }

    /// Fail requests that take longer than `timeout`.
    ///
    /// If the next layer takes more than `timeout` to respond to a request,
    /// processing is terminated and an error is returned.
    pub fn timeout(self, timeout: Duration) -> ServiceBuilder<Chain<TimeoutLayer, L>> {
        self.layer(TimeoutLayer::new(timeout))
    }

    /// Create a `LayeredMakeService` from the composed layers and transport `MakeService`.
    pub fn make_service<M, Target, Request>(self, mk: M) -> LayeredMakeService<M, L, Request>
    where
        M: MakeService<Target, Request>,
    {
        LayeredMakeService::new(mk, self.layer)
    }

    /// Wrap the service `S` with the layers.
    pub fn service<S, Request>(self, service: S) -> Result<L::Service, L::LayerError>
    where
        L: Layer<S, Request>,
        S: Service<Request>,
    {
        self.layer.layer(service)
    }
}
