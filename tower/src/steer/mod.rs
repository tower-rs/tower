//! This module provides functionality to aid managing routing requests between [`Service`]s.
//!
//! # Example
//!
//! [`Steer`] can for example be used to create a router, akin to what you might find in web
//! frameworks.
//!
//! Here, `GET /` will be sent to the `root` service, while all other requests go to `not_found`.
//!
//! ```rust
//! # use std::task::{Context, Poll};
//! # use tower_service::Service;
//! # use futures_util::future::{ready, Ready, poll_fn};
//! # use tower::steer::Steer;
//! # use tower::service_fn;
//! # use tower::util::BoxService;
//! # use tower::ServiceExt;
//! # use std::convert::Infallible;
//! use http::{Request, Response, StatusCode, Method};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Service that responds to `GET /`
//! let root = service_fn(|req: Request<String>| async move {
//!     # assert_eq!(req.uri().path(), "/");
//!     let res = Response::new("Hello, World!".to_string());
//!     Ok::<_, Infallible>(res)
//! });
//! // We have to box the service so its type gets erased and we can put it in a `Vec` with other
//! // services
//! let root = BoxService::new(root);
//!
//! // Service that responds with `404 Not Found` to all requests
//! let not_found = service_fn(|req: Request<String>| async move {
//!     let res = Response::builder()
//!         .status(StatusCode::NOT_FOUND)
//!         .body(String::new())
//!         .expect("response is valid");
//!     Ok::<_, Infallible>(res)
//! });
//! // Box that as well
//! let not_found = BoxService::new(not_found);
//!
//! let mut svc = Steer::new(
//!     // All services we route between
//!     vec![root, not_found],
//!     // How we pick which service to send the request to
//!     |req: &Request<String>, _services: &[_]| {
//!         if req.method() == Method::GET && req.uri().path() == "/" {
//!             0 // Index of `root`
//!         } else {
//!             1 // Index of `not_found`
//!         }
//!     },
//! );
//!
//! // This request will get sent to `root`
//! let req = Request::get("/").body(String::new()).unwrap();
//! let res = svc.ready().await?.call(req).await?;
//! assert_eq!(res.into_body(), "Hello, World!");
//!
//! // This request will get sent to `not_found`
//! let req = Request::get("/does/not/exist").body(String::new()).unwrap();
//! let res = svc.ready().await?.call(req).await?;
//! assert_eq!(res.status(), StatusCode::NOT_FOUND);
//! assert_eq!(res.into_body(), "");
//! #
//! # Ok(())
//! # }
//! ```
use std::task::{Context, Poll};
use std::{collections::VecDeque, fmt, marker::PhantomData};
use tower_service::Service;

/// This is how callers of [`Steer`] tell it which `Service` a `Req` corresponds to.
pub trait Picker<S, Req> {
    /// Return an index into the iterator of `Service` passed to [`Steer::new`].
    fn pick(&mut self, r: &Req, services: &[S]) -> usize;
}

impl<S, F, Req> Picker<S, Req> for F
where
    F: Fn(&Req, &[S]) -> usize,
{
    fn pick(&mut self, r: &Req, services: &[S]) -> usize {
        self(r, services)
    }
}

/// [`Steer`] manages a list of [`Service`]s which all handle the same type of request.
///
/// An example use case is a sharded service.
/// It accepts new requests, then:
/// 1. Determines, via the provided [`Picker`], which [`Service`] the request coresponds to.
/// 2. Waits (in [`Service::poll_ready`]) for *all* services to be ready.
/// 3. Calls the correct [`Service`] with the request, and returns a future corresponding to the
///    call.
///
/// Note that [`Steer`] must wait for all services to be ready since it can't know ahead of time
/// which [`Service`] the next message will arrive for, and is unwilling to buffer items
/// indefinitely. This will cause head-of-line blocking unless paired with a [`Service`] that does
/// buffer items indefinitely, and thus always returns [`Poll::Ready`]. For example, wrapping each
/// component service with a [`Buffer`] with a high enough limit (the maximum number of concurrent
/// requests) will prevent head-of-line blocking in [`Steer`].
///
/// [`Buffer`]: crate::buffer::Buffer
pub struct Steer<S: Service<Req>, F, Req> {
    router: F,
    services: Vec<S>,
    tokens: Vec<Option<S::Token>>,
    _phantom: PhantomData<Req>,
}

impl<S, F, Req> Steer<S, F, Req>
where
    S: Service<Req>,
{
    /// Make a new [`Steer`] with a list of [`Service`]'s and a [`Picker`].
    ///
    /// Note: the order of the [`Service`]'s is significant for [`Picker::pick`]'s return value.
    pub fn new(services: impl IntoIterator<Item = S>, router: F) -> Self {
        let services: Vec<_> = services.into_iter().collect();
        let not_ready: VecDeque<_> = services.iter().enumerate().map(|(i, _)| i).collect();
        let tokens: Vec<_> = services.iter().map(|_| None).collect();
        Self {
            router,
            services,
            tokens,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct Token<T>(Vec<T>);

impl<S, Req, F> Service<Req> for Steer<S, F, Req>
where
    S: Service<Req>,
    F: Picker<S, Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Token = Token<S::Token>;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Token, Self::Error>> {
        // must wait for *all* services to be ready.
        // this will cause head-of-line blocking unless the underlying services are always ready.
        let mut all = true;
        for (token, service) in self.tokens.iter_mut().zip(&mut self.services) {
            if token.is_some() {
                continue;
            }
            match service.poll_ready(cx)? {
                Poll::Ready(t) => *token = Some(t),
                Poll::Pending => all = false,
            }
        }
        if all {
            Poll::Ready(Ok(Token(
                self.tokens
                    .iter_mut()
                    .map(|token| token.take().unwrap())
                    .collect(),
            )))
        } else {
            Poll::Pending
        }
    }

    fn call(&mut self, token: Self::Token, req: Req) -> Self::Future {
        let idx = self.router.pick(&req, &self.services[..]);
        let cl = &mut self.services[idx];
        let token = token.0[idx];
        cl.call(token, req)
    }
}

impl<S, F, Req> Clone for Steer<S, F, Req>
where
    S: Service<Req> + Clone,
    F: Clone,
{
    fn clone(&self) -> Self {
        let mut tokens = Vec::new();
        tokens.resize_with(self.tokens.len(), Default::default);
        Self {
            router: self.router.clone(),
            services: self.services.clone(),
            tokens,
            _phantom: PhantomData,
        }
    }
}

impl<S, F, Req> fmt::Debug for Steer<S, F, Req>
where
    S: Service<Req> + fmt::Debug,
    F: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self {
            router,
            services,
            tokens,
            _phantom,
        } = self;
        f.debug_struct("Steer")
            .field("router", router)
            .field("services", services)
            .field("tokens", tokens.iter())
            .finish()
    }
}
