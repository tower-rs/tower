//! This crate provides functionality to aid managing routing requests between Tower [`Service`]s.
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
//! # use tower_steer::SteerService;
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
//!     let mut s = SteerService::new(
//!         vec![MyService(0), MyService(1)],
//!         // one service handles strings with uppercase first letters. the other handles the rest.
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

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// This is how callers of [`SteerService`] tell it which `Service` a `Req` corresponds to.
pub trait Picker<Req> {
    /// Return an index into the iterator of `Service` passed to [`SteerService::new`].
    fn pick(&mut self, r: &Req) -> usize;
}

impl<F, Req> Picker<Req> for F
where
    F: Fn(&Req) -> usize,
{
    fn pick(&mut self, r: &Req) -> usize {
        self(r)
    }
}

/// `SteerService` manages a list of `Service`s.
///
/// It accepts new requests, then:
/// 1. Determines, via the provided [`Picker`], which `Service` the request coresponds to.
/// 2. Waits (in `poll_ready`) for *all* services to be ready.
/// 3. Calls the correct `Service` with the request, and returns a future corresponding to the
///    call.
#[derive(Debug)]
pub struct SteerService<S, F, Req> {
    router: F,
    // tuple of is_ready, service
    cls: Vec<(bool, S)>,
    _phantom: std::marker::PhantomData<Req>,
}

impl<S, F, Req> SteerService<S, F, Req>
where
    S: Service<Req, Error = StdError>,
    S::Future: 'static,
{
    /// Make a new [`SteerService`] with a list of `Service`s and a `Picker`.
    ///
    /// Note: the order of the `Service`s is significant for [`Picker::pick`]'s return value.
    pub fn new(cls: impl IntoIterator<Item = S>, router: F) -> Self {
        let cls: Vec<_> = cls.into_iter().map(|s| (false, s)).collect();
        Self {
            router,
            cls,
            _phantom: Default::default(),
        }
    }
}

impl<S, Req, T, F> Service<Req> for SteerService<S, F, Req>
where
    S: Service<Req, Response = T, Error = StdError>,
    S::Future: 'static,
    F: Picker<Req>,
{
    type Response = T;
    type Error = StdError;
    type Future = Pin<Box<dyn Future<Output = Result<T, StdError>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        use futures_util::ready;
        // must wait for *all* services to be ready.
        // this will cause head-of-line blocking unless the underlying services are always ready.
        for (is_ready, serv) in &mut self.cls {
            if *is_ready {
                continue;
            } else {
                ready!(serv.poll_ready(cx))?;
                *is_ready = true;
            }
        }

        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let idx = self.router.pick(&req);
        let (ready, cl): &mut (bool, S) = &mut self.cls[idx];
        assert!(*ready);
        let fut = cl.call(req);
        *ready = false;
        Box::pin(fut)
    }
}
