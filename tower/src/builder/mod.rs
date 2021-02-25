//! Builder types to compose layers and services

use tower_layer::{Identity, Layer, Stack};

use std::fmt;

/// Declaratively construct [`Service`] values.
///
/// [`ServiceBuilder`] provides a [builder-like interface][builder] for composing
/// layers to be applied to a [`Service`].
///
/// # Service
///
/// A [`Service`] is a trait representing an asynchronous function of a request
/// to a response. It is similar to `async fn(Request) -> Result<Response, Error>`.
///
/// A [`Service`] is typically bound to a single transport, such as a TCP
/// connection.  It defines how _all_ inbound or outbound requests are handled
/// by that connection.
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
/// # // this (and other) doctest is ignored because we don't have a way
/// # // to say that it should only be run with cfg(feature = "...")
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # #[cfg(all(feature = "buffer", feature = "limit"))]
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
/// # #[cfg(all(feature = "buffer", feature = "limit"))]
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
/// A [`Service`] stack with a single layer:
///
/// ```
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # #[cfg(feature = "limit")]
/// # use tower::limit::concurrency::ConcurrencyLimitLayer;
/// # #[cfg(feature = "limit")]
/// # async fn wrap<S>(svc: S) where S: Service<(), Error = &'static str> + 'static + Send, S::Future: Send {
/// ServiceBuilder::new()
///     .concurrency_limit(5)
///     .service(svc);
/// # ;
/// # }
/// ```
///
/// A [`Service`] stack with _multiple_ layers that contain rate limiting,
/// in-flight request limits, and a channel-backed, clonable [`Service`]:
///
/// ```
/// # use tower::Service;
/// # use tower::builder::ServiceBuilder;
/// # use std::time::Duration;
/// # #[cfg(all(feature = "buffer", feature = "limit"))]
/// # async fn wrap<S>(svc: S) where S: Service<(), Error = &'static str> + 'static + Send, S::Future: Send {
/// ServiceBuilder::new()
///     .buffer(5)
///     .concurrency_limit(5)
///     .rate_limit(5, Duration::from_secs(1))
///     .service(svc);
/// # ;
/// # }
/// ```
///
/// [`Service`]: crate::Service
#[derive(Clone)]
pub struct ServiceBuilder<L> {
    layer: L,
}

impl Default for ServiceBuilder<Identity> {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceBuilder<Identity> {
    /// Create a new [`ServiceBuilder`].
    pub fn new() -> Self {
        ServiceBuilder {
            layer: Identity::new(),
        }
    }
}

impl<L> ServiceBuilder<L> {
    /// Add a new layer `T` into the [`ServiceBuilder`].
    ///
    /// This wraps the inner service with the service provided by a user-defined
    /// [`Layer`]. The provided layer must implement the [`Layer`] trait.
    ///
    /// [`Layer`]: crate::Layer
    pub fn layer<T>(self, layer: T) -> ServiceBuilder<Stack<T, L>> {
        ServiceBuilder {
            layer: Stack::new(layer, self.layer),
        }
    }

    /// Optionally add a new layer `T` into the [`ServiceBuilder`].
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use tower::Service;
    /// # use tower::builder::ServiceBuilder;
    /// # use tower::timeout::TimeoutLayer;
    /// # async fn wrap<S>(svc: S) where S: Service<(), Error = &'static str> + 'static + Send, S::Future: Send {
    /// # let timeout = Some(Duration::new(10, 0));
    /// // Apply a timeout if configured
    /// ServiceBuilder::new()
    ///     .option_layer(timeout.map(TimeoutLayer::new))
    ///     .service(svc)
    /// # ;
    /// # }
    /// ```
    #[cfg(feature = "util")]
    #[cfg_attr(docsrs, doc(cfg(feature = "util")))]
    pub fn option_layer<T>(
        self,
        layer: Option<T>,
    ) -> ServiceBuilder<Stack<crate::util::Either<T, Identity>, L>> {
        self.layer(crate::util::option_layer(layer))
    }

    /// Add a [`Layer`] built from a function that accepts a service and returns another service.
    ///
    /// See the documentation for [`layer_fn`] for more details.
    ///
    /// [`layer_fn`]: crate::layer::layer_fn
    pub fn layer_fn<F>(self, f: F) -> ServiceBuilder<Stack<crate::layer::LayerFn<F>, L>> {
        self.layer(crate::layer::layer_fn(f))
    }

    /// Buffer requests when when the next layer is not ready.
    ///
    /// This wraps the inner service with an instance of the [`Buffer`]
    /// middleware.
    ///
    /// [`Buffer`]: crate::buffer
    #[cfg(feature = "buffer")]
    #[cfg_attr(docsrs, doc(cfg(feature = "buffer")))]
    pub fn buffer<Request>(
        self,
        bound: usize,
    ) -> ServiceBuilder<Stack<crate::buffer::BufferLayer<Request>, L>> {
        self.layer(crate::buffer::BufferLayer::new(bound))
    }

    /// Limit the max number of in-flight requests.
    ///
    /// A request is in-flight from the time the request is received until the
    /// response future completes. This includes the time spent in the next
    /// layers.
    ///
    /// This wraps the inner service with an instance of the
    /// [`ConcurrencyLimit`] middleware.
    ///
    /// [`ConcurrencyLimit`]: crate::limit::concurrency
    #[cfg(feature = "limit")]
    #[cfg_attr(docsrs, doc(cfg(feature = "limit")))]
    pub fn concurrency_limit(
        self,
        max: usize,
    ) -> ServiceBuilder<Stack<crate::limit::ConcurrencyLimitLayer, L>> {
        self.layer(crate::limit::ConcurrencyLimitLayer::new(max))
    }

    /// Drop requests when the next layer is unable to respond to requests.
    ///
    /// Usually, when a service or middleware does not have capacity to process a
    /// request (i.e., [`poll_ready`] returns [`Pending`]), the caller waits until
    /// capacity becomes available.
    ///
    /// [`LoadShed`] immediately responds with an error when the next layer is
    /// out of capacity.
    ///
    /// This wraps the inner service with an instance of the [`LoadShed`]
    /// middleware.
    ///
    /// [`LoadShed`]: crate::load_shed
    /// [`poll_ready`]: crate::Service::poll_ready
    /// [`Pending`]: std::task::Poll::Pending
    #[cfg(feature = "load-shed")]
    #[cfg_attr(docsrs, doc(cfg(feature = "load-shed")))]
    pub fn load_shed(self) -> ServiceBuilder<Stack<crate::load_shed::LoadShedLayer, L>> {
        self.layer(crate::load_shed::LoadShedLayer::new())
    }

    /// Limit requests to at most `num` per the given duration.
    ///
    /// This wraps the inner service with an instance of the [`RateLimit`]
    /// middleware.
    ///
    /// [`RateLimit`]: crate::limit::rate
    #[cfg(feature = "limit")]
    #[cfg_attr(docsrs, doc(cfg(feature = "limit")))]
    pub fn rate_limit(
        self,
        num: u64,
        per: std::time::Duration,
    ) -> ServiceBuilder<Stack<crate::limit::RateLimitLayer, L>> {
        self.layer(crate::limit::RateLimitLayer::new(num, per))
    }

    /// Retry failed requests according to the given [retry policy][policy].
    ///
    /// `policy` determines which failed requests will be retried. It must
    /// implement the [`retry::Policy`][policy] trait.
    ///
    /// This wraps the inner service with an instance of the [`Retry`]
    /// middleware.
    ///
    /// [`Retry`]: crate::retry
    /// [policy]: crate::retry::Policy
    #[cfg(feature = "retry")]
    #[cfg_attr(docsrs, doc(cfg(feature = "retry")))]
    pub fn retry<P>(self, policy: P) -> ServiceBuilder<Stack<crate::retry::RetryLayer<P>, L>> {
        self.layer(crate::retry::RetryLayer::new(policy))
    }

    /// Fail requests that take longer than `timeout`.
    ///
    /// If the next layer takes more than `timeout` to respond to a request,
    /// processing is terminated and an error is returned.
    ///
    /// This wraps the inner service with an instance of the [`timeout`]
    /// middleware.
    ///
    /// [`timeout`]: crate::timeout
    #[cfg(feature = "timeout")]
    #[cfg_attr(docsrs, doc(cfg(feature = "timeout")))]
    pub fn timeout(
        self,
        timeout: std::time::Duration,
    ) -> ServiceBuilder<Stack<crate::timeout::TimeoutLayer, L>> {
        self.layer(crate::timeout::TimeoutLayer::new(timeout))
    }

    /// Conditionally reject requests based on `predicate`.
    ///
    /// `predicate` must implement the [`Predicate`] trait.
    ///
    /// This wraps the inner service with an instance of the [`Filter`]
    /// middleware.
    ///
    /// [`Filter`]: crate::filter
    /// [`Predicate`]: crate::filter::Predicate
    #[cfg(feature = "filter")]
    #[cfg_attr(docsrs, doc(cfg(feature = "filter")))]
    pub fn filter<P>(
        self,
        predicate: P,
    ) -> ServiceBuilder<Stack<crate::filter::FilterLayer<P>, L>> {
        self.layer(crate::filter::FilterLayer::new(predicate))
    }

    /// Conditionally reject requests based on an asynchronous `predicate`.
    ///
    /// `predicate` must implement the [`AsyncPredicate`] trait.
    ///
    /// This wraps the inner service with an instance of the [`AsyncFilter`]
    /// middleware.
    ///
    /// [`AsyncFilter`]: crate::filter::AsyncFilter
    /// [`AsyncPredicate`]: crate::filter::AsyncPredicate
    #[cfg(feature = "filter")]
    #[cfg_attr(docsrs, doc(cfg(feature = "filter")))]
    pub fn filter_async<P>(
        self,
        predicate: P,
    ) -> ServiceBuilder<Stack<crate::filter::AsyncFilterLayer<P>, L>> {
        self.layer(crate::filter::AsyncFilterLayer::new(predicate))
    }

    /// Map one request type to another.
    ///
    /// This wraps the inner service with an instance of the [`MapRequest`]
    /// middleware.
    ///
    /// # Examples
    ///
    /// Changing the type of a request:
    ///
    /// ```rust
    /// use tower::ServiceBuilder;
    /// use tower::ServiceExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), ()> {
    /// // Suppose we have some `Service` whose request type is `String`:
    /// let string_svc = tower::service_fn(|request: String| async move {
    ///     println!("request: {}", request);
    ///     Ok(())
    /// });
    ///
    /// // ...but we want to call that service with a `usize`. What do we do?
    ///
    /// let usize_svc = ServiceBuilder::new()
    ///      // Add a middlware that converts the request type to a `String`:
    ///     .map_request(|request: usize| format!("{}", request))
    ///     // ...and wrap the string service with that middleware:
    ///     .service(string_svc);
    ///
    /// // Now, we can call that service with a `usize`:
    /// usize_svc.oneshot(42).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Modifying the request value:
    ///
    /// ```rust
    /// use tower::ServiceBuilder;
    /// use tower::ServiceExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), ()> {
    /// // A service that takes a number and returns it:
    /// let svc = tower::service_fn(|request: usize| async move {
    ///    Ok(request)
    /// });
    ///
    /// let svc = ServiceBuilder::new()
    ///      // Add a middleware that adds 1 to each request
    ///     .map_request(|request: usize| request + 1)
    ///     .service(svc);
    ///
    /// let response = svc.oneshot(1).await?;
    /// assert_eq!(response, 2);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`MapRequest`]: crate::util::MapRequest
    #[cfg(feature = "util")]
    #[cfg_attr(docsrs, doc(cfg(feature = "util")))]
    pub fn map_request<F, R1, R2>(
        self,
        f: F,
    ) -> ServiceBuilder<Stack<crate::util::MapRequestLayer<F>, L>>
    where
        F: FnMut(R1) -> R2 + Clone,
    {
        self.layer(crate::util::MapRequestLayer::new(f))
    }

    /// Map one response type to another.
    ///
    /// This wraps the inner service with an instance of the [`MapResponse`]
    /// middleware.
    ///
    /// See the documentation for the [`map_response` combinator] for details.
    ///
    /// [`MapResponse`]: crate::util::MapResponse
    /// [`map_response` combinator]: crate::util::ServiceExt::map_response
    #[cfg(feature = "util")]
    #[cfg_attr(docsrs, doc(cfg(feature = "util")))]
    pub fn map_response<F>(
        self,
        f: F,
    ) -> ServiceBuilder<Stack<crate::util::MapResponseLayer<F>, L>> {
        self.layer(crate::util::MapResponseLayer::new(f))
    }

    /// Map one error type to another.
    ///
    /// This wraps the inner service with an instance of the [`MapErr`]
    /// middleware.
    ///
    /// See the documentation for the [`map_err` combinator] for details.
    ///
    /// [`MapErr`]: crate::util::MapErr
    /// [`map_err` combinator]: crate::util::ServiceExt::map_err
    #[cfg(feature = "util")]
    #[cfg_attr(docsrs, doc(cfg(feature = "util")))]
    pub fn map_err<F>(self, f: F) -> ServiceBuilder<Stack<crate::util::MapErrLayer<F>, L>> {
        self.layer(crate::util::MapErrLayer::new(f))
    }

    /// Composes a function that transforms futures produced by the service.
    ///
    /// This wraps the inner service with an instance of the [`MapFutureLayer`] middleware.
    ///
    /// See the documentation for the [`map_future`] combinator for details.
    ///
    /// [`MapFutureLayer`]: crate::util::MapFutureLayer
    /// [`map_future`]: crate::util::ServiceExt::map_future
    #[cfg(feature = "util")]
    #[cfg_attr(docsrs, doc(cfg(feature = "util")))]
    pub fn map_future<F>(self, f: F) -> ServiceBuilder<Stack<crate::util::MapFutureLayer<F>, L>> {
        self.layer(crate::util::MapFutureLayer::new(f))
    }

    /// Apply a function after the service, regardless of whether the future
    /// succeeds or fails.
    ///
    /// This wraps the inner service with an instance of the [`Then`]
    /// middleware.
    ///
    /// This is similar to the [`map_response`] and [`map_err`] functions,
    /// except that the *same* function is invoked when the service's future
    /// completes, whether it completes successfully or fails. This function
    /// takes the [`Result`] returned by the service's future, and returns a
    /// [`Result`].
    ///
    /// See the documentation for the [`then` combinator] for details.
    ///
    /// [`Then`]: crate::util::Then
    /// [`then` combinator]: crate::util::ServiceExt::then
    /// [`map_response`]: ServiceBuilder::map_response
    /// [`map_err`]: ServiceBuilder::map_err
    #[cfg(feature = "util")]
    #[cfg_attr(docsrs, doc(cfg(feature = "util")))]
    pub fn then<F>(self, f: F) -> ServiceBuilder<Stack<crate::util::ThenLayer<F>, L>> {
        self.layer(crate::util::ThenLayer::new(f))
    }

    /// Returns the underlying `Layer` implementation.
    pub fn into_inner(self) -> L {
        self.layer
    }

    /// Wrap the service `S` with the middleware provided by this
    /// [`ServiceBuilder`]'s [`Layer`]'s, returning a new [`Service`].
    ///
    /// [`Layer`]: crate::Layer
    /// [`Service`]: crate::Service
    pub fn service<S>(&self, service: S) -> L::Service
    where
        L: Layer<S>,
    {
        self.layer.layer(service)
    }

    /// Wrap the async function `F` with the middleware provided by this [`ServiceBuilder`]'s
    /// [`Layer`]s, returning a new [`Service`].
    ///
    /// This is a convenience method which is equivalent to calling
    /// [`ServiceBuilder::service`] with a [`service_fn`], like this:
    ///
    /// ```rust
    /// # use tower::{ServiceBuilder, service_fn};
    /// # async fn handler_fn(_: ()) -> Result<(), ()> { Ok(()) }
    /// # let _ = {
    /// ServiceBuilder::new()
    ///     // ...
    ///     .service(service_fn(handler_fn))
    /// # };
    /// ```
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use tower::{ServiceBuilder, ServiceExt, BoxError, service_fn};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), BoxError> {
    /// async fn handle(request: &'static str) -> Result<&'static str, BoxError> {
    ///    Ok(request)
    /// }
    ///
    /// let svc = ServiceBuilder::new()
    ///     .buffer(1024)
    ///     .timeout(Duration::from_secs(10))
    ///     .service_fn(handle);
    ///
    /// let response = svc.oneshot("foo").await?;
    ///
    /// assert_eq!(response, "foo");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`Layer`]: crate::Layer
    /// [`Service`]: crate::Service
    /// [`service_fn`]: crate::service_fn
    #[cfg(feature = "util")]
    #[cfg_attr(docsrs, doc(cfg(feature = "util")))]
    pub fn service_fn<F>(self, f: F) -> L::Service
    where
        L: Layer<crate::util::ServiceFn<F>>,
    {
        self.service(crate::util::service_fn(f))
    }
}

impl<L: fmt::Debug> fmt::Debug for ServiceBuilder<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ServiceBuilder").field(&self.layer).finish()
    }
}
