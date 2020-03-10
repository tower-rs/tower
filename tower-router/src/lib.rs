//! This crate provides functionality to aid managing routing requests between different Tower [`Service`]s.
//!
//! # Example
//! ```rust
//! # use std::{
//! #    pin::Pin,
//! #    task::{Context, Poll},
//! # };
//! # use tower_service::Service;
//! # use futures_util::future::{ready, Ready, poll_fn};
//! # use futures_util::never::Never;
//! # use tower_router::ServiceRouter;
//! type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;
//! struct MyService(u8);
//!
//! impl Service<String> for MyService {
//!     type Response = ();
//!     type Error = StdError;
//!     type Future = Ready<Result<(), Self::Error>>;
//!
//!     fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//!         Poll::Ready(Ok(()))
//!     }
//!
//!     fn call(&mut self, req: String) -> Self::Future {
//!         println!("{}: {}", self.0, req);
//!         ready(Ok(()))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     // one service handles strings with
//!     let mut s = ServiceRouter::new(
//!         vec![MyService(0), MyService(1)],
//!         |r: &String| if r.chars().next().unwrap().is_uppercase() { 0 } else { 1 },
//!     );
//!
//!     let reqs = vec!["A", "b", "C", "d"];
//!     let reqs: Vec<String> = reqs.into_iter().map(String::from).collect();
//!     for r in reqs {
//!         poll_fn(|cx| s.poll_ready(cx)).await.unwrap();
//!         s.call(r).await;
//!     }
//! }
//! ```
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
use std::sync::Arc;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::{oneshot, Mutex};
use tower_service::Service;

type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// This is how callers of [`ServiceRouter`] tell it which `Service` a `Req` corresponds to.
pub trait Router<Req> {
    /// Return an index into the iterator of `Service` passed to [`new`].
    fn route(&mut self, r: &Req) -> usize;
}

impl<F, Req> Router<Req> for F
where
    F: Fn(&Req) -> usize,
{
    fn route(&mut self, r: &Req) -> usize {
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
    inner: Arc<Mutex<ServiceGroup<OneshotService<S>, (oneshot::Sender<T>, Req)>>>,
}

impl<S, Req: Send + 'static, T, F> ServiceRouter<S, Req, T, F>
where
    S: Service<Req, Response = T, Error = StdError> + Send + 'static,
    S::Future: Send + 'static,
    T: Send + 'static,
{
    /// Make a new [`ServiceRouter`] with a list of `Service`s and a `Router`.
    ///
    /// Note: the order of the `Service`s is significant for [`Router::route`]'s return value.
    pub fn new(cls: impl IntoIterator<Item = S>, router: F) -> Self {
        let cls = cls.into_iter().map(|c| OneshotService(c));
        let sg = Arc::new(Mutex::new(ServiceGroup::new(cls)));
        let sg_stream = sg.clone();

        // spawn a task that will drive inner
        tokio::spawn(async move {
            // for each future inner Stream gives us, spawn it
            loop {
                let next = {
                    use futures_util::stream::StreamExt;
                    let mut sg = sg_stream.lock().await;
                    (*sg).next().await
                };

                match next {
                    // got the next future. spawn it.
                    Some(Ok(fut)) => {
                        tokio::spawn(fut);
                    }
                    // no futures yet.
                    None => {
                        tokio::task::yield_now().await;
                        continue;
                    }
                    Some(Err(_)) => {
                        break;
                    }
                }
            }
        });

        Self { router, inner: sg }
    }

    /// Get the length of the request queue for service `idx`
    ///
    /// Calls [`ServiceGroup::len`].
    pub async fn len(&self, idx: usize) -> usize {
        self.inner.lock().await.len(idx)
    }

    /// Get the lengths of all the request queues.
    ///
    /// Calls [`ServiceGroup::len_all`].
    pub async fn len_all(&self) -> Vec<usize> {
        self.inner.lock().await.len_all()
    }
}

impl<S, Req: Send + 'static, T, F> Service<Req> for ServiceRouter<S, Req, T, F>
where
    S: Service<Req, Response = T, Error = StdError> + Send + 'static,
    S::Future: Send + 'static,
    F: Router<Req>,
    T: Send,
{
    type Response = T;
    type Error = StdError;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<T, StdError>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // because we infinitely buffer, we are always ready
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let idx = self.router.route(&req);
        let (tx, rx) = tokio::sync::oneshot::channel::<T>();

        // avoid capturing self.router unnecessarily
        fn do_call<S, T, Req: Send + 'static>(
            inner: Arc<Mutex<ServiceGroup<OneshotService<S>, (oneshot::Sender<T>, Req)>>>,
            idx: usize,
            tx: oneshot::Sender<T>,
            rx: oneshot::Receiver<T>,
            req: Req,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<T, StdError>> + Send + 'static>>
        where
            S: Service<Req, Response = T, Error = StdError> + Send + 'static,
            S::Future: Send + 'static,
            T: Send + 'static,
        {
            tokio::spawn(async move {
                let mut sg = inner.lock().await;
                sg.push(idx, (tx, req));
            });
            Box::pin(async move {
                let res = rx.await?;
                Ok(res)
            })
        }

        do_call(self.inner.clone(), idx, tx, rx, req)
    }
}

/// Send the results of S on a oneshot instead of returning them.
///
/// ```rust
/// # use std::{
/// #    pin::Pin,
/// #    task::{Context, Poll},
/// # };
/// # use tower_service::Service;
/// # use futures_util::future::{ready, Ready, poll_fn};
/// # use futures_util::never::Never;
/// # use tower_router::{OneshotService, ServiceGroup};
/// type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;
/// struct MyService(u8);
///
/// impl Service<String> for MyService {
///     type Response = ();
///     type Error = StdError;
///     type Future = Ready<Result<(), Self::Error>>;
///
///     fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
///         Poll::Ready(Ok(()))
///     }
///
///     fn call(&mut self, req: String) -> Self::Future {
///         println!("{}: {}", self.0, req);
///         ready(Ok(()))
///     }
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let s = MyService(0);
///     let s = OneshotService(s);
///     let mut sg = ServiceGroup::new(vec![s]);
///     let (tx, rx) = tokio::sync::oneshot::channel();
///     sg.push(0, (tx, String::from("a")));
///
///     use futures_util::stream::StreamExt;
///     let fut = sg.next().await.unwrap().unwrap();
///     tokio::spawn(fut);
///     rx.await;
/// }
/// ```
#[derive(Debug)]
pub struct OneshotService<S>(pub S);

impl<S, Req, T> Service<(oneshot::Sender<T>, Req)> for OneshotService<S>
where
    S: Service<Req, Response = T, Error = StdError>,
    S::Future: Send + 'static,
    T: Send + 'static,
{
    type Response = ();
    type Error = S::Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<(), S::Error>> + Send + 'static>>;

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
    S::Future: Send + 'static,
{
    type Item = Result<
        Pin<Box<dyn std::future::Future<Output = Result<T, StdError>> + Send + 'static>>,
        StdError,
    >;

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
