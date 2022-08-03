//! Future types

use super::{Policy, Retry};
use futures_core::ready;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower_service::Service;

pin_project! {
    /// The [`Future`] returned by a [`Retry`] service.
    #[derive(Debug)]
    pub struct ResponseFuture<P, S, Request>
    where
        P: Policy<Request, S::Response, S::Error>,
        S: Service<Request>,
    {
        #[pin]
        retry: Retry<P, S>,
        #[pin]
        state: State<S::Future, P::Future, Request, S::Response, S::Error>,
    }
}

pin_project! {
    #[project = StateProj]
    #[derive(Debug)]
    enum State<F, P, Req, Res, E> {
        // Polling the future from [`Service::call`]
        Called {
            #[pin]
            future: F
        },
        // Polling the future from [`Policy::retry`]
        Checking {
            result: Option<Result<Res, E>>,
            #[pin]
            checking: P
        },
        // Polling [`Service::poll_ready`] after [`Checking`] was OK.
        Retrying { result: Option<Result<Res, E>>, req: Option<Req>},
    }
}

impl<P, S, Request> ResponseFuture<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error>,
    S: Service<Request>,
{
    pub(crate) fn new(retry: Retry<P, S>, future: S::Future) -> ResponseFuture<P, S, Request> {
        ResponseFuture {
            retry,
            state: State::Called { future },
        }
    }
}

impl<P, S, Request> Future for ResponseFuture<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error> + Clone + Unpin,
    S: Service<Request> + Clone,
{
    type Output = Result<S::Response, S::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match this.state.as_mut().project() {
                StateProj::Called { future } => {
                    let mut result = ready!(future.poll(cx));
                    let checking = this
                        .retry
                        .as_mut()
                        .project()
                        .policy
                        .as_mut()
                        .retry(&mut result);
                    // Some(checking) => {
                    this.state.set(State::Checking {
                        result: Some(result),
                        checking,
                    });
                    // }
                    // None => return Poll::Ready(result),
                }
                StateProj::Checking { result, checking } => {
                    let req = ready!(checking.poll(cx));

                    let result = result.take();
                    this.state.set(State::Retrying { result, req });
                }
                StateProj::Retrying { result, req } => {
                    // NOTE: we assume here that
                    //
                    //   this.retry.poll_ready()
                    //
                    // is equivalent to
                    //
                    //   this.retry.service.poll_ready()
                    //
                    // we need to make that assumption to avoid adding an Unpin bound to the Policy
                    // in Ready to make it Unpin so that we can get &mut Ready as needed to call
                    // poll_ready on it.
                    ready!(this.retry.as_mut().project().service.poll_ready(cx))?;

                    let req = match req.take() {
                        Some(req) => req,
                        None => {
                            return Poll::Ready(
                                result
                                    .take()
                                    .expect("This is a bug there should be a result"),
                            )
                        }
                    };

                    this.state.set(State::Called {
                        future: this.retry.as_mut().project().service.call(req),
                    });
                }
            }
        }
    }
}
