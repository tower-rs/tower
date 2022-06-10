use futures_core::ready;
use pin_project_lite::pin_project;
use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::{Call, Service};

pin_project! {
    /// A [`Future`] consuming a [`Service`] and request, waiting until the [`Service`]
    /// is ready, and then calling [`Service::call`] with the request, and
    /// waiting for that [`Future`].
    #[derive(Debug)]
    pub struct Oneshot<S, Req, F> {
        svc: S,
        #[pin]
        state: State<Req, F>,
    }
}

pin_project! {
    #[project = StateProj]
    enum State<Req, F> {
        NotReady {
            req: Option<Req>,
        },
        Called {
            #[pin]
            fut: F,
        },
        Done,
    }
}

impl<Req, F> State<Req, F> {
    fn not_ready(req: Req) -> Self {
        Self::NotReady { req: Some(req) }
    }

    fn called(fut: F) -> Self {
        Self::Called { fut }
    }
}

impl<Req, F> fmt::Debug for State<Req, F>
where
    Req: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::NotReady { req: Some(req) } => {
                f.debug_tuple("State::NotReady").field(req).finish()
            }

            State::NotReady { req: None, .. } => unreachable!(),
            State::Called { .. } => f.debug_tuple("State::Called").field(&"S::Future").finish(),
            State::Done => f.debug_tuple("State::Done").finish(),
        }
    }
}

impl<S, Req, F> Oneshot<S, Req, F>
where
    S: for<'a> Service<'a, Req, Future = F>,
{
    #[allow(missing_docs)]
    pub fn new(svc: S, req: Req) -> Self {
        Oneshot {
            svc,
            state: State::not_ready(req),
        }
    }
}

impl<S, Req, Rsp, Err, F> Future for Oneshot<S, Req, F>
where
    S: for<'a> Service<'a, Req, Error = Err, Response = Rsp, Future = F>,
    F: Future<Output = Result<Rsp, Err>>,
{
    type Output = Result<Rsp, Err>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        loop {
            match this.state.as_mut().project() {
                StateProj::NotReady { req } => {
                    let mut call = ready!(this.svc.poll_ready(cx))?;
                    let f = call.call(req.take().expect("already called"));
                    this.state.set(State::called(f));
                }
                StateProj::Called { fut } => {
                    let res = ready!(fut.poll(cx))?;
                    this.state.set(State::Done);
                    return Poll::Ready(Ok(res));
                }
                StateProj::Done => panic!("polled after complete"),
            }
        }
    }
}
