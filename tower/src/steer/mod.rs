//! This module provides functionality to aid managing routing requests between Tower [`Service`]s.
//!
//! # Example
//! ```rust
//! # use std::task::{Context, Poll};
//! # use tower_service::Service;
//! # use futures_util::future::{ready, Ready, poll_fn};
//! # use tower::steer::Steer;
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
//!     let mut s = Steer::new(
//!         vec![MyService(0), MyService(1)],
//!         // one service handles strings with uppercase first letters. the other handles the rest.
//!         |r: &String, _: &[_]| if r.chars().next().unwrap().is_uppercase() { 0 } else { 1 },
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
use std::collections::VecDeque;
use std::task::{Context, Poll};
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

/// `Steer` manages a list of `Service`s which all handle the same type of request.
///
/// An example use case is a sharded service.
/// It accepts new requests, then:
/// 1. Determines, via the provided [`Picker`], which `Service` the request coresponds to.
/// 2. Waits (in `poll_ready`) for *all* services to be ready.
/// 3. Calls the correct `Service` with the request, and returns a future corresponding to the
///    call.
///
/// Note that `Steer` must wait for all services to be ready since it can't know ahead of time
/// which `Service` the next message will arrive for, and is unwilling to buffer items
/// indefinitely. This will cause head-of-line blocking unless paired with a `Service` that does
/// buffer items indefinitely, and thus always returns `Poll::Ready`. For example, wrapping each
/// component service with a `tower-buffer` with a high enough limit (the maximum number of
/// concurrent requests) will prevent head-of-line blocking in `Steer`.
#[derive(Debug)]
pub struct Steer<S, F, Req> {
    router: F,
    services: Vec<S>,
    not_ready: VecDeque<usize>,
    _phantom: std::marker::PhantomData<Req>,
}

impl<S, F, Req> Steer<S, F, Req> {
    /// Make a new [`Steer`] with a list of `Service`s and a `Picker`.
    ///
    /// Note: the order of the `Service`s is significant for [`Picker::pick`]'s return value.
    pub fn new(services: impl IntoIterator<Item = S>, router: F) -> Self {
        let services: Vec<_> = services.into_iter().collect();
        let not_ready: VecDeque<_> = services.iter().enumerate().map(|(i, _)| i).collect();
        Self {
            router,
            services,
            not_ready,
            _phantom: Default::default(),
        }
    }
}

impl<S, Req, F> Service<Req> for Steer<S, F, Req>
where
    S: Service<Req>,
    F: Picker<S, Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            // must wait for *all* services to be ready.
            // this will cause head-of-line blocking unless the underlying services are always ready.
            if self.not_ready.is_empty() {
                return Poll::Ready(Ok(()));
            } else {
                if let Poll::Pending = self.services[self.not_ready[0]].poll_ready(cx)? {
                    return Poll::Pending;
                }

                self.not_ready.pop_front();
            }
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        assert!(
            self.not_ready.is_empty(),
            "Steer must wait for all services to be ready. Did you forget to call poll_ready()?"
        );

        let idx = self.router.pick(&req, &self.services[..]);
        let cl = &mut self.services[idx];
        self.not_ready.push_back(idx);
        cl.call(req)
    }
}
