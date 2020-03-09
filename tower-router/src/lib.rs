//! This crate provides functionality to aid managing routing requests between different Tower [`Service`]s.
#![deny(missing_docs)]
#![warn(unreachable_pub)]
#![warn(missing_copy_implementations)]
#![warn(trivial_casts)]
#![warn(trivial_numeric_casts)]
#![warn(unused_extern_crates)]
#![warn(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
#![allow(clippy::type_complexity)]

use futures_util::stream::Stream;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tower_service::Service;

type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// This is how callers of [`ServiceRouter`] tell it which `Service` a `Req` corresponds to.
pub trait Router<Req> {
    /// Return an index into the iterator of `Service` passed to [`new`].
    fn route(&self, r: &Req) -> usize;
}

impl<F, Req> Router<Req> for F
where
    F: Fn(&Req) -> usize,
{
    fn route(&self, r: &Req) -> usize {
        self(r)
    }
}

/// `ServiceRouter` manages a list of `Service`s.
///
/// It accepts new requests, then:
/// 1. Determines, via the provided [`Router`], which `Service` the request coresponds to.
/// Then it returns a future that:
/// 2. Waits for that `Service` to be ready (using [`ServiceGroup`]).
/// 3. Calls the `Service` with the request, and waits for the response.
/// 4. Resolves to the response of the call.
///
/// Unlike `tower-buffer`, `ServiceRouter` does not require `S` to `impl Clone`, but like
/// `tower-buffer` it uses a [`oneshot`] to manage requests.
#[derive(Debug)]
pub struct ServiceRouter<S, Req, T: 'static, F> {
    router: F,
    inner: ServiceGroup<OneshotService<S>, (oneshot::Sender<T>, Req)>,
}

impl<S, Req, T, F> ServiceRouter<S, Req, T, F>
where
    S: Service<Req, Response = T, Error = StdError>,
    S::Future: 'static,
{
    /// Make a new [`ServiceRouter`] with a list of `Service`s and a `Router`.
    ///
    /// Note: the order of the `Service`s is significant for [`Router::route`]'s return value.
    pub fn new(cls: impl IntoIterator<Item = S>, router: F) -> Self {
        let cls = cls.into_iter().map(|c| OneshotService(c));
        Self {
            router,
            inner: ServiceGroup::new(cls),
        }
    }

    /// Get the length of the request queue for service `idx`
    ///
    /// Calls [`ServiceGroup::len`].
    pub fn len(&self, idx: usize) -> usize {
        self.inner.len(idx)
    }

    /// Get the lengths of all the request queues.
    ///
    /// Calls [`ServiceGroup::len_all`].
    pub fn len_all(&self) -> Vec<usize> {
        self.inner.len_all()
    }
}

impl<S, Req, T, F> Service<Req> for ServiceRouter<S, Req, T, F>
where
    S: Service<Req, Response = T, Error = StdError>,
    S::Future: 'static,
    F: Router<Req>,
{
    type Response = T;
    type Error = StdError;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<T, StdError>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // because we infinitely buffer, we are always ready
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let idx = self.router.route(&req);
        let (tx, rx) = tokio::sync::oneshot::channel::<T>();
        self.inner.push(idx, (tx, req));
        Box::pin(async move {
            let res = rx.await?;
            Ok(res)
        })
    }
}

/// Send the results of S on a oneshot instead of just returning them.
///
/// This lets us construct a future that returns the right response to each request.
#[derive(Debug)]
struct OneshotService<S>(S);

impl<S, Req, T> Service<(oneshot::Sender<T>, Req)> for OneshotService<S>
where
    S: Service<Req, Response = T>,
    S::Future: 'static,
    T: 'static,
{
    type Response = ();
    type Error = S::Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<(), S::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx)
    }

    fn call(&mut self, req: (oneshot::Sender<T>, Req)) -> Self::Future {
        let (tx, req) = req;
        let fut = self.0.call(req);
        Box::pin(async move {
            let res: T = fut.await?;
            // unwrap_or_else -> panic to avoid constraining `Error` and `T` with Debug
            tx.send(res)
                .unwrap_or_else(|_| panic!("receiver hung up on Req result"));
            Ok(())
        })
    }
}

/// `ServiceGroup` polls a group of services for them to become ready, buffering incoming requests in
/// the meanwhile.
///
/// To avoid head-of-line blocking, `ServiceGroup` buffers an unbounded amount of requests to each
/// `Service`.
///
/// As clients become ready, it returns futures (via `impl Stream`) which perform the request on
/// the correct `Service`. It does *not* `.await` these `Future`s - this is left to the caller.
/// This way, the caller is able to `.await` several returned `Future`s at once, instead of
/// blocking on the completion of one at a time.
///
/// It will `poll_ready` only the services which have pending requests. Otherwise, we could
/// end up repeatedly calling `poll_ready` on a service for which there are no requests.
#[pin_project::pin_project]
#[derive(Debug)]
pub struct ServiceGroup<S, Req> {
    cls: Vec<S>,
    pending: Vec<VecDeque<Req>>,
    pending_now: Vec<usize>,
}

impl<S, Req, T> ServiceGroup<S, Req>
where
    S: Service<Req, Response = T, Error = StdError>,
    S::Future: 'static,
{
    /// Make a new [`ServiceGroup`].
    pub fn new(cls: impl IntoIterator<Item = S>) -> Self {
        let cls: Vec<_> = cls.into_iter().collect();
        let pending = cls.iter().map(|_| Default::default()).collect();
        Self {
            cls,
            pending,
            pending_now: vec![],
        }
    }

    /// Push a new request onto service `idx`.
    pub fn push(&mut self, idx: usize, r: Req) {
        if self.pending[idx].is_empty() {
            self.pending_now.push(idx);
        }

        self.pending[idx].push_back(r);
    }

    /// Get the length of the request queue for service `idx`
    pub fn len(&self, idx: usize) -> usize {
        self.pending[idx].len()
    }

    /// Get the lengths of all the request queues.
    pub fn len_all(&self) -> Vec<usize> {
        (0..self.cls.len()).map(|i| self.len(i)).collect()
    }
}

// Why return a `Future` instead of allowing the caller to make their own `Future` (i.e., do
// `.call()` themselves)? Because returning `&mut dyn Service` is a massive pain in terms of
// lifetimes and I couldn't figure out how to make it work.
impl<S, Req, T> Stream for ServiceGroup<S, Req>
where
    S: Service<Req, Response = T, Error = StdError>,
    S::Future: 'static,
{
    type Item = Result<Pin<Box<dyn std::future::Future<Output = Result<T, StdError>>>>, StdError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // project to split the borrow
        let mut this = self.project();
        let pending: &mut _ = &mut this.pending;
        let pending_now: &mut _ = &mut this.pending_now;
        let cls: &mut _ = &mut this.cls;

        if pending_now.is_empty() {
            return Poll::Ready(None);
        }

        // rotate for poll fairness
        pending_now.rotate_left(1);
        // see if any of them are ready
        let mut ready_now: Option<usize> = None;
        let mut ready_now_idx = 0;
        for i in 0..pending_now.len() {
            let idx = pending_now[i];
            match cls[idx].poll_ready(cx) {
                Poll::Pending => continue,
                Poll::Ready(Ok(())) => {
                    ready_now.replace(idx);
                    ready_now_idx = i;
                    break;
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e.into()))),
            }
        }

        match ready_now {
            None => return Poll::Pending, // there are waiting requests, but all of the clients are pending.
            Some(idx) => {
                // cls[idx] is ready.
                let cl = &mut cls[idx];
                let r = pending[idx].pop_front().expect("only polled active conns");

                // get a future which does the call
                let fut = cl.call(r);

                if pending[idx].is_empty() {
                    // no more messages, yank from pending_now
                    pending_now.remove(ready_now_idx);
                }

                Poll::Ready(Some(Ok(Box::pin(fut))))
            }
        }
    }
}
