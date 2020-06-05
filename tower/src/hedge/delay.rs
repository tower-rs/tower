use futures_util::ready;
use pin_project::pin_project;
use std::time::Duration;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// A policy which specifies how long each request should be delayed for.
pub trait Policy<Request> {
    fn delay(&self, req: &Request) -> Duration;
}

/// A middleware which delays sending the request to the underlying service
/// for an amount of time specified by the policy.
#[derive(Debug)]
pub struct Delay<P, S> {
    policy: P,
    service: S,
}

#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<Request, S, F> {
    service: S,
    #[pin]
    state: State<Request, F>,
}

#[pin_project(project = StateProj)]
#[derive(Debug)]
enum State<Request, F> {
    Delaying(#[pin] tokio::time::Delay, Option<Request>),
    Called(#[pin] F),
}

impl<P, S> Delay<P, S> {
    pub fn new<Request>(policy: P, service: S) -> Self
    where
        P: Policy<Request>,
        S: Service<Request> + Clone,
        S::Error: Into<crate::BoxError>,
    {
        Delay { policy, service }
    }
}

impl<Request, P, S> Service<Request> for Delay<P, S>
where
    P: Policy<Request>,
    S: Service<Request> + Clone,
    S::Error: Into<crate::BoxError>,
{
    type Response = S::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<Request, S, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx).map_err(|e| e.into())
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let deadline = tokio::time::Instant::now() + self.policy.delay(&request);
        let mut cloned = self.service.clone();
        // Pass the original service to the ResponseFuture and keep the cloned service on self.
        let orig = {
            std::mem::swap(&mut cloned, &mut self.service);
            cloned
        };
        ResponseFuture {
            service: orig,
            state: State::Delaying(tokio::time::delay_until(deadline), Some(request)),
        }
    }
}

impl<Request, S, F, T, E> Future for ResponseFuture<Request, S, F>
where
    F: Future<Output = Result<T, E>>,
    E: Into<crate::BoxError>,
    S: Service<Request, Future = F, Response = T, Error = E>,
{
    type Output = Result<T, crate::BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match this.state.as_mut().project() {
                StateProj::Delaying(delay, req) => {
                    ready!(delay.poll(cx));
                    let req = req.take().expect("Missing request in delay");
                    let fut = this.service.call(req);
                    this.state.set(State::Called(fut));
                }
                StateProj::Called(fut) => {
                    return fut.poll(cx).map_err(Into::into);
                }
            };
        }
    }
}
