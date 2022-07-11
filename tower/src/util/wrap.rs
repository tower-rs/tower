//! Middleware that decorates a [`Service`] with pre/post functions.

use std::{fmt, future::Future, marker::PhantomData, pin::Pin, task::Poll};

use futures_core::ready;
use pin_project_lite::pin_project;
use tower_layer::Layer;
use tower_service::Service;

/// Middleware that decorates a [`Service`] with pre/post functions.
///
/// Service returned by the [`wrap`] and [`decorate`] combinators.
///
/// [`wrap`]: fn@crate::util::ServiceExt::wrap
/// [`decorate`]: fn@crate::util::ServiceExt::decorate
#[derive(Clone)]
pub struct Wrap<S, Pre, Post> {
    inner: S,
    pre: Pre,
    post: Post,
}

impl<S, F, F2> Wrap<S, F, F2> {
    /// Creates a new `Wrap` service with separate async pre/post functions.
    ///
    /// The `pre` function is any function that looks like
    /// <code>async [FnMut]\(Request) -> [Result]<(SRequest, T), [Result]<Response, Err>></code>
    /// where `Request` is the request given to the `Wrap` service.
    ///
    /// * If this function outputs <code>[Ok]\((SRequest, T))</code> the `SRequest` is passed to
    ///   the inner service and the `T` is retained as shared state. When the inner service outputs
    ///   a result, this result is passed to `post` along with the shared `T`.
    /// * If this function returns <code>[Err]\(result)</code> the result is output by the `Wrap`
    ///   service without calling the inner service or the `post` function.
    ///
    /// The `post` function is any function that looks like
    /// <code>async [FnOnce]\(Res, T) -> [Result]<Response, Err></code> where `Res` is the result
    /// returned by the inner service, `T` is the shared state provided by `pre`, and `Response`
    /// and `Err` match the types used in `pre`. The returned [`Result`] is the overall result from
    /// the wrapped service.
    ///
    /// See also [`new`](Self::new) for when you just need to share state across the inner service
    /// and don't need asynchronous functions or short-circuiting.
    pub fn decorate(inner: S, pre: F, post: F2) -> Self {
        Self { inner, pre, post }
    }

    /// Creates a new `Wrap` service with a synchronous pre function that returns a post function.
    ///
    /// The given function is any function that looks like
    /// <code>[FnMut]\(&Request) -> [FnOnce]\(Response) -> T</code> where `Request` is the request
    /// given to the `Wrap` service, `Response` is the response returned from the inner service,
    /// and `T` is the response returned from the `Wrap` service. If the inner service returns an
    /// error the error is output directly without being given to the post function.
    pub fn new<Request>(
        inner: S,
        f: F,
    ) -> Wrap<S, Pre<Request, S::Response, S::Error, F>, Post<F2, S::Error>>
    where
        F: FnMut(&Request) -> F2,
        S: Service<Request>,
    {
        Wrap {
            inner,
            pre: Pre {
                f,
                _marker: PhantomData,
            },
            post: Post {
                _marker: PhantomData,
            },
        }
    }
}

impl<F, F2> Wrap<(), F, F2> {
    /// Returns a new [`Layer`] that produces [`Wrap`] services.
    ///
    /// This is a convenience function that simply calls [`WrapLayer::new`].
    pub fn layer<Req, Res, E>(f: F) -> WrapLayer<Pre<Req, Res, E, F>, Post<F2, E>>
    where
        F: FnMut(&Req) -> F2,
    {
        WrapLayer::new(f)
    }
}

/// A [`Layer`] that produces a [`Wrap`] service.
#[derive(Clone)]
pub struct WrapLayer<Pre, Post> {
    pre: Pre,
    post: Post,
}

impl<F, F2> WrapLayer<F, F2> {
    /// Creates a new `WrapLayer` layer with separate async pre/post functions.
    ///
    /// The `pre` function is any function that looks like
    /// <code>async [FnMut]\(Request) -> [Result]<(SRequest, T), [Result]<Response, Err>></code>
    /// where `Request` is the request given to the `Wrap` service.
    ///
    /// * If this function outputs <code>[Ok]\((SRequest, T))</code> the `SRequest` is passed to
    ///   the inner service and the `T` is retained as shared state. When the inner service outputs
    ///   a result, this result is passed to `post` along with the shared `T`.
    /// * If this function returns <code>[Err]\(result)</code> the result is output by the `Wrap`
    ///   service without calling the inner service or the `post` function.
    ///
    /// The `post` function is any function that looks like
    /// <code>async [FnOnce]\(Res, T) -> [Result]<Response, Err></code> where `Res` is the result
    /// returned by the inner service, `T` is the shared state provided by `pre`, and `Response`
    /// and `Err` match the types used in `pre`. The returned [`Result`] is the overall result from
    /// the wrapped service.
    ///
    /// See also [`new`](Self::new) for when you just need to share state across the inner service
    /// and don't need asynchronous functions or short-circuiting.
    pub fn decorate(pre: F, post: F2) -> Self {
        Self { pre, post }
    }

    /// Creates a new `WrapLayer` layer with a synchronous pre function that returns a post
    /// function.
    ///
    /// The given function is any function that looks like
    /// <code>[FnMut]\(&Request) -> [FnOnce]\(Response) -> T</code> where `Request` is the request
    /// given to the `Wrap` service, `Response` is the response returned from the inner service,
    /// and `T` is the response returned from the `Wrap` service. If the inner service returns an
    /// error the error is output directly without being given to the post function.
    pub fn new<Request, Response, E>(f: F) -> WrapLayer<Pre<Request, Response, E, F>, Post<F2, E>>
    where
        F: FnMut(&Request) -> F2,
    {
        WrapLayer {
            pre: Pre {
                f,
                _marker: PhantomData,
            },
            post: Post {
                _marker: PhantomData,
            },
        }
    }
}

impl<S, Pre, Post> Layer<S> for WrapLayer<Pre, Post>
where
    Pre: Clone,
    Post: Clone,
{
    type Service = Wrap<S, Pre, Post>;

    fn layer(&self, inner: S) -> Self::Service {
        Wrap {
            inner,
            pre: self.pre.clone(),
            post: self.post.clone(),
        }
    }
}

impl<S, Req, Pre, Post, Res, E> Service<Req> for Wrap<S, Pre, Post>
where
    Pre: PreFn<Req, Response = Res, Error = E>,
    S: Service<Pre::Request> + Clone,
    S::Error: Into<E>,
    Post: PostFn<S::Response, S::Error, Pre::Value, Response = Res, Error = E> + Clone,
{
    type Response = Res;
    type Error = E;
    type Future = WrapFuture<S, S::Future, Pre::Future, Post, Post::Future, Pre::Value>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        WrapFuture {
            state: State::Pre {
                future: self.pre.call(req),
                inner: self.inner.clone(),
                post: self.post.clone(),
            },
        }
    }
}

pin_project! {
    /// Response future for [`Wrap`].
    pub struct WrapFuture<S, SFut, PreFut, Post, PostFut, T> {
        #[pin]
        state: State<S, SFut, PreFut, Post, PostFut, T>,
    }
}

pin_project! {
    #[project = StateProj]
    #[project_replace = StateOwned]
    enum State<S, SFut, PreFut, Post, PostFut, T> {
        Pre {
            #[pin]
            future: PreFut,
            inner: S,
            post: Post
        },
        Inner {
            #[pin]
            future: SFut,
            post: Post,
            value: T,
        },
        Post {
            #[pin]
            future: PostFut,
        },
        Complete
    }
}

impl<S, SReq, PreFut, Res, E, Post, T> Future
    for WrapFuture<S, S::Future, PreFut, Post, Post::Future, T>
where
    PreFut: Future<Output = Result<(SReq, T), Result<Res, E>>>,
    S: Service<SReq>,
    S::Error: Into<E>,
    Post: PostFn<S::Response, S::Error, T, Response = Res, Error = E>,
{
    type Output = Result<Res, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(loop {
            let mut state = self.as_mut().project().state;
            match state.as_mut().project() {
                StateProj::Pre { future, .. } => match ready!(future.poll(cx)) {
                    Ok((req, value)) => match state.as_mut().project_replace(State::Complete) {
                        StateOwned::Pre {
                            mut inner, post, ..
                        } => state.set(State::Inner {
                            future: inner.call(req),
                            post,
                            value,
                        }),
                        _ => unreachable!(),
                    },
                    Err(res) => {
                        state.set(State::Complete);
                        break res.map_err(Into::into);
                    }
                },
                StateProj::Inner { future, .. } => {
                    let result = ready!(future.poll(cx));
                    match state.as_mut().project_replace(State::Complete) {
                        StateOwned::Inner { post, value, .. } => state.set(State::Post {
                            future: post.call(result, value),
                        }),
                        _ => unreachable!(),
                    }
                }
                StateProj::Post { future } => {
                    let res = ready!(future.poll(cx));
                    state.set(State::Complete);
                    break res;
                }
                StateProj::Complete => {
                    panic!("WrapFuture must not be polled after it returned `Poll::Ready`")
                }
            }
        })
    }
}

/// A trait that represents pre functions for use with [`Wrap`].
///
/// This is implemented for all
/// <code>async [FnMut]\(Req) -> [Result]<(SReq, T), [Result]<Response, Err>></code> where `SReq`
/// is the request type passed to the inner service, `T` is the shared state passed to the
/// [`PostFn`], and <code>[Result]<Response, Err></code> is a short-circuit result that bypasses
/// the wrapped service and post-fn.
pub trait PreFn<Req> {
    /// Requests passed to the inner service.
    type Request;
    /// Shared values passed to the [`PostFn`].
    type Value;
    /// Responses given by the service.
    ///
    /// This matches the corresponding [`PostFn::Response`].
    type Response;
    /// Errors produced by the service.
    ///
    /// This matches the corresponding [`PostFn::Error`].
    type Error;
    /// The future result used to either call the inner service or short-circuit.
    type Future: Future<
        Output = Result<(Self::Request, Self::Value), Result<Self::Response, Self::Error>>,
    >;

    /// Invokes the function.
    fn call(&mut self, request: Req) -> Self::Future;
}

impl<F, Req, T, Fut, SReq, Res, Err> PreFn<Req> for F
where
    F: FnMut(Req) -> Fut,
    Fut: Future<Output = Result<(SReq, T), Result<Res, Err>>>,
{
    type Request = SReq;
    type Value = T;
    type Response = Res;
    type Error = Err;
    type Future = Fut;

    fn call(&mut self, request: Req) -> Self::Future {
        self(request)
    }
}

/// A trait that represents post functions for use with [`Wrap`].
///
/// This is implemented for all
/// <code>async [FnMut]\([Result]<Res, E>, T) -> [Result]<Response, Error></code> where
/// <code>[Result]<Res, E></code> is the output from the inner service, `T` is the shared state
/// returned from the [`PreFn`], and <code>[Result]<Response, Error></code> is the overall result
/// from the wrapped service.
pub trait PostFn<Res, E, T> {
    /// Responses given by the service.
    type Response;
    /// Errors produced by the service.
    type Error;
    /// The future response value.
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    /// Invokes the function.
    fn call(self, result: Result<Res, E>, value: T) -> Self::Future;
}

impl<F, Res, E, T, Fut, Ok, Err> PostFn<Res, E, T> for F
where
    F: FnOnce(Result<Res, E>, T) -> Fut,
    Fut: Future<Output = Result<Ok, Err>>,
{
    type Response = Ok;
    type Error = Err;
    type Future = Fut;

    fn call(self, result: Result<Res, E>, value: T) -> Self::Future {
        self(result, value)
    }
}

/// Helper type for use with [`Wrap::new`].
///
/// This type is an adaptor from [`Wrap::new`]'s synchronous pre function to the more generic
/// asynchronous failable pre function given to [`Wrap::decorate`].
#[allow(missing_debug_implementations)]
pub struct Pre<Req, Res, E, F> {
    f: F,
    _marker: PhantomData<fn(Req) -> (Req, Res, E)>,
}

impl<F, Req, Post, Res, E> PreFn<Req> for Pre<Req, Res, E, F>
where
    F: FnMut(&Req) -> Post,
{
    type Request = Req;
    type Value = Post;
    type Response = Res;
    type Error = E;
    type Future = WrapPreFuture<Req, Post, Res, E>;

    fn call(&mut self, request: Req) -> Self::Future {
        let f2 = (self.f)(&request);
        WrapPreFuture::new(futures_util::future::ready(Ok((request, f2))))
    }
}

opaque_future! {
    /// Future returned by the [`PreFn`] impl for [`Pre`].
    pub type WrapPreFuture<Req, Post, Res, E> =
        futures_util::future::Ready<Result<(Req, Post), Result<Res, E>>>;
}

/// Helper type for use with [`Wrap::new`].
///
/// This type is an adaptor from [`Wrap::new`]'s synchronous post function to the more generic
/// asynchronous failable post function given to [`Wrap::decorate`].
#[allow(missing_debug_implementations)]
pub struct Post<F, E> {
    _marker: PhantomData<fn(F, E) -> E>,
}

impl<F, Res, E, T> PostFn<Res, E, F> for Post<F, E>
where
    F: FnOnce(Res) -> T,
{
    type Response = T;
    type Error = E;
    type Future = WrapPostFuture<T, E>;

    fn call(self, result: Result<Res, E>, value: F) -> Self::Future {
        WrapPostFuture::new(futures_util::future::ready(result.map(value)))
    }
}

opaque_future! {
    /// Future returned by the [`PostFn`] impl for [`Post`].
    pub type WrapPostFuture<T, E> = futures_util::future::Ready<Result<T, E>>;
}

impl<S: fmt::Debug, Pre, Post> fmt::Debug for Wrap<S, Pre, Post> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wrap")
            .field("inner", &self.inner)
            .field("pre", &format_args!("{}", std::any::type_name::<Pre>()))
            .field("post", &format_args!("{}", std::any::type_name::<Post>()))
            .finish()
    }
}

impl<Pre, Post> fmt::Debug for WrapLayer<Pre, Post> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WrapLayer")
            .field("pre", &format_args!("{}", std::any::type_name::<Pre>()))
            .field("post", &format_args!("{}", std::any::type_name::<Post>()))
            .finish()
    }
}

impl<Req, Res, E, F: Clone> Clone for Pre<Req, Res, E, F> {
    fn clone(&self) -> Self {
        Self {
            f: self.f.clone(),
            _marker: PhantomData,
        }
    }
}

impl<F, E> Clone for Post<F, E> {
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}
