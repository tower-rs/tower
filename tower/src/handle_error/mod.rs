// TODO(david): write docs if we go forward with this
#![allow(missing_docs)]

use futures_core::ready;
use pin_project::pin_project;
use std::{
    convert::Infallible,
    fmt,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;

pub struct HandleErrorLayer<F, R> {
    f: F,
    _request: PhantomData<fn() -> R>,
}

impl<F, R> Clone for HandleErrorLayer<F, R>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            f: self.f.clone(),
            _request: PhantomData,
        }
    }
}

impl<F, R> fmt::Debug for HandleErrorLayer<F, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandleErrorLayer")
            .field("f", &format_args!("{}", std::any::type_name::<F>()))
            .finish()
    }
}

impl<F, R> HandleErrorLayer<F, R> {
    pub fn new(f: F) -> Self {
        HandleErrorLayer {
            f,
            _request: PhantomData,
        }
    }
}

impl<S, R, F> Layer<S> for HandleErrorLayer<F, R>
where
    F: Clone,
    S: Service<R>,
{
    type Service = HandleError<S, R, F>;

    fn layer(&self, inner: S) -> Self::Service {
        HandleError::new(inner, self.f.clone())
    }
}

#[derive(Clone)]
pub struct HandleError<S, R, F>
where
    S: Service<R>,
{
    inner: S,
    f: F,
    poll_ready_error: Option<S::Error>,
}

impl<S, R, F> fmt::Debug for HandleError<S, R, F>
where
    S: Service<R> + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandleError")
            .field("inner", &self.inner)
            .field("f", &format_args!("{}", std::any::type_name::<F>()))
            .finish()
    }
}

impl<S, R, F> HandleError<S, R, F>
where
    S: Service<R>,
{
    pub fn new(inner: S, f: F) -> Self
    where
        S: Service<R>,
    {
        Self {
            inner,
            f,
            poll_ready_error: None,
        }
    }

    pub fn layer(f: F) -> HandleErrorLayer<F, R> {
        HandleErrorLayer::new(f)
    }
}

impl<S, R, F> Service<R> for HandleError<S, R, F>
where
    S: Service<R>,
    F: FnOnce(S::Error) -> S::Response + Clone,
{
    type Response = S::Response;
    type Error = Infallible;
    type Future = ResponseFuture<S::Future, F, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Err(err) = ready!(self.inner.poll_ready(cx)) {
            self.poll_ready_error = Some(err);
        }

        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: R) -> Self::Future {
        if let Some(poll_ready_error) = self.poll_ready_error.take() {
            return ResponseFuture::error(poll_ready_error, self.f.clone());
        }

        ResponseFuture::future(self.inner.call(req), self.f.clone())
    }
}

#[pin_project]
pub struct ResponseFuture<Fut, F, E> {
    #[pin]
    kind: Kind<Fut, F, E>,
}

impl<Fut, F, E> ResponseFuture<Fut, F, E> {
    pub(super) fn future(future: Fut, f: F) -> Self {
        Self {
            kind: Kind::Future { future, f: Some(f) },
        }
    }

    pub(super) fn error(err: E, f: F) -> Self {
        Self {
            kind: Kind::Error(Some((err, f))),
        }
    }
}

#[pin_project(project = KindProj)]
enum Kind<Fut, F, E> {
    Future {
        #[pin]
        future: Fut,
        f: Option<F>,
    },
    Error(Option<(E, F)>),
}

impl<Fut, F, T, E> Future for ResponseFuture<Fut, F, E>
where
    Fut: Future<Output = Result<T, E>>,
    F: FnOnce(E) -> T,
{
    type Output = Result<T, Infallible>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().kind.project() {
            KindProj::Future { future, f } => match ready!(future.poll(cx)) {
                Ok(res) => Poll::Ready(Ok(res)),
                Err(err) => {
                    let f = f.take().unwrap();
                    Poll::Ready(Ok(f(err)))
                }
            },
            KindProj::Error(pair) => {
                let (err, f) = pair.take().unwrap();
                Poll::Ready(Ok(f(err)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoxError, ServiceBuilder, ServiceExt};
    use std::time::Duration;

    #[tokio::test]
    async fn fail_in_call() {
        struct FailInCall;

        impl Service<()> for FailInCall {
            type Response = String;
            type Error = &'static str;
            type Future = futures::future::Ready<Result<Self::Response, Self::Error>>;

            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _req: ()) -> Self::Future {
                futures::future::ready(Err("error"))
            }
        }

        let svc = ServiceBuilder::new()
            // initially fails with `&'static str`
            .check_service::<FailInCall, _, _, &'static str>()
            // handle errors
            .layer(HandleErrorLayer::new(|err: &'static str| {
                format!("error handled: {}", err)
            }))
            // now never fails
            .check_service::<FailInCall, _, _, Infallible>()
            .service(FailInCall);

        let err = svc.oneshot(()).await.unwrap();

        assert_eq!(err, "error handled: error");
    }

    #[tokio::test]
    async fn fail_in_poll_ready() {
        struct FailInPollReady;

        impl Service<()> for FailInPollReady {
            type Response = String;
            type Error = &'static str;
            type Future = futures::future::Ready<Result<Self::Response, Self::Error>>;

            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Err("poll_ready error"))
            }

            fn call(&mut self, _req: ()) -> Self::Future {
                unreachable!()
            }
        }

        let svc = ServiceBuilder::new()
            // initially fails with `&'static str`
            .check_service::<FailInPollReady, _, _, &'static str>()
            // handle errors
            .layer(HandleErrorLayer::new(|err: &'static str| {
                format!("error handled: {}", err)
            }))
            // now never fails
            .check_service::<FailInPollReady, _, _, Infallible>()
            .service(FailInPollReady);

        let err = svc.oneshot(()).await.unwrap();

        assert_eq!(err, "error handled: poll_ready error");
    }

    #[tokio::test]
    async fn works_in_large_stack() {
        let svc = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(|err: BoxError| {
                format!("error handled: {}", err)
            }))
            .load_shed()
            .concurrency_limit(1024)
            .rate_limit(100, Duration::from_millis(100))
            .timeout(Duration::from_secs(30))
            .service_fn(|_req: ()| async { Ok::<_, BoxError>("ok".to_string()) });

        // as long as this types checks we are good
        let _: Result<String, Infallible> = svc.oneshot(()).await;
    }
}
