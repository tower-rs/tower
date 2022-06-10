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
    pub struct Oneshot<S, Req, F>
    where
        S: for<'a> Service<'a, Req, Future = F>,
    {
        svc: S,
        #[pin]
        state: State<Req, F>,
    }
}

pin_project! {
    #[project = StateProj]
    enum State<Req, F> {
        NotReady(Option<Req>),
        Called(#[pin] F),
        Done,
    }
}

impl<S: Service<Req>, Req> State<S, Req> {
    fn not_ready(svc: S, req: Option<Req>) -> Self {
        Self::NotReady { svc, req }
    }

    fn called(fut: S::Future) -> Self {
        Self::Called { fut }
    }


impl<Req, F> fmt::Debug for State<Req, F>
where
    Req: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::NotReady(req) => f.debug_tuple("State::NotReady").field(req).finish(),
            State::Called(_) => f.debug_tuple("State::Called").field(&"S::Future").finish(),
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
            state: State::NotReady(Some(req)),
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
                StateProj::NotReady(req) => {
                    let mut call = ready!(this.svc.poll_ready(cx))?;
                    let f = call.call(req.take().expect("already called"));
                    this.state.set(State::Called(f));
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
