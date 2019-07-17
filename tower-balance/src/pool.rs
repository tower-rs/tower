//! This module defines a load-balanced pool of services that adds new services when load is high.
//!
//! The pool uses `poll_ready` as a signal indicating whether additional services should be spawned
//! to handle the current level of load. Specifically, every time `poll_ready` on the inner service
//! returns `Ready`, [`Pool`] consider that a 0, and every time it returns `NotReady`, [`Pool`]
//! considers it a 1. [`Pool`] then maintains an [exponential moving
//! average](https://en.wikipedia.org/wiki/Moving_average#Exponential_moving_average) over those
//! samples, which gives an estimate of how often the underlying service has been ready when it was
//! needed "recently" (see [`Builder::urgency`]). If the service is loaded (see
//! [`Builder::loaded_above`]), a new service is created and added to the underlying [`Balance`].
//! If the service is underutilized (see [`Builder::underutilized_below`]) and there are two or
//! more services, then the latest added service is removed. In either case, the load estimate is
//! reset to its initial value (see [`Builder::initial`] to prevent services from being rapidly
//! added or removed.
#![deny(missing_docs)]

use crate::error;
use futures::{future, try_ready, Async, Future, Poll};
use tower_load::Load;
use tower_ready_cache::ReadyCache;
use tower_service::Service;
use tower_util::MakeService;

/// A dynamically sized pool of `Service` instances.
pub struct Pool<MS, Request>
where
    MS: MakeService<(), Request>,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    next_id: usize,
    min_services: usize,
    max_services: usize,
    make_service: MS,
    cache: ReadyCache<usize, MakeReady<MS, Request>, Request>,
}

enum MakeReady<MS, Request>
where
    MS: MakeService<(), Request>,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    Make(MS::Future),
    Ready(Option<MS::Service>),
}

impl<MS, Request> Pool<MS, Request>
where
    MS: MakeService<(), Request>,
    MS::Service: Load,
    <MS::Service as Load>::Metric: std::fmt::Debug,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    /// Construct a new dynamically sized `Pool`.
    ///
    /// If many calls to `poll_ready` return `NotReady`, `new_service` is used to
    /// construct another `Service` that is then added to the load-balanced pool.
    /// If many calls to `poll_ready` succeed, the most recently added `Service`
    /// is dropped from the pool.
    pub fn new(capacity: std::ops::Range<usize>, make_service: MS) -> Self {
        Self {
            make_service,
            min_services: capacity.start,
            max_services: capacity.end,
            next_id: 0,
            cache: ReadyCache::default(),
        }
    }

    fn push_new_service(&mut self) {
        let id = self.next_id;
        self.next_id += 1;

        let fut = self.make_service.make_service(());
        self.cache.push_service(id, MakeReady::Make(fut));
    }
}

impl<MS, Req> Service<Req> for Pool<MS, Req>
where
    MS: MakeService<(), Req>,
    MS::Service: Load,
    <MS::Service as Load>::Metric: std::fmt::Debug,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    type Response = <MS::Service as Service<Req>>::Response;
    type Error = error::Error;
    type Future = future::MapErr<
        <MS::Service as Service<Req>>::Future,
        fn(<MS::Service as Service<Req>>::Error) -> error::Error,
    >;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        loop {
            let _ = self.cache.poll_pending();

            if self.cache.len() < self.min_services {
                while self.cache.len() < self.min_services {
                    self.push_new_service();
                }
                let _ = self.cache.poll_pending();
            }

            if self.cache.ready_len() > 0 {
                if let Ok(Async::Ready(())) = self.cache.poll_next_ready_index(0) {
                    return Ok(Async::Ready(()));
                }
            }

            if self.cache.len() == self.max_services {
                return Ok(Async::NotReady);
            }

            self.push_new_service();
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        debug_assert!(self.cache.ready_len() > 0);
        self.cache.call_next_ready(req)
    }
}

impl<MS, Request> Future for MakeReady<MS, Request>
where
    MS: MakeService<(), Request>,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    type Item = MS::Service;
    type Error = error::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            *self = match *self {
                MakeReady::Make(ref mut fut) => {
                    let svc = try_ready!(fut.poll().map_err(Into::into));
                    MakeReady::Ready(Some(svc))
                }
                MakeReady::Ready(ref mut svc) => {
                    try_ready!(svc
                        .as_mut()
                        .expect("polled after ready")
                        .poll_ready()
                        .map_err(Into::into));
                    return Ok(Async::Ready(svc.take().expect("polled after ready")));
                }
            };
        }
    }
}

impl<MS, Request> Service<Request> for MakeReady<MS, Request>
where
    MS: MakeService<(), Request>,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    type Response = MS::Response;
    type Error = error::Error;
    type Future =
        future::MapErr<<MS::Service as Service<Request>>::Future, fn(MS::Error) -> Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        loop {
            *self = match self {
                MakeReady::Make(ref mut fut) => {
                    let svc = try_ready!(fut.poll().map_err(Into::into));
                    MakeReady::Ready(Some(svc))
                }
                MakeReady::Ready(ref mut svc) => {
                    return svc
                        .as_mut()
                        .expect("invalid state: missing service")
                        .poll_ready()
                        .map_err(Into::into);
                }
            };
        }
    }

    fn call(&mut self, req: Request) -> Self::Future {
        match self {
            MakeReady::Make(_) => panic!("not ready"),
            MakeReady::Ready(ref mut svc) => {
                return svc
                    .as_mut()
                    .expect("missing service")
                    .call(req)
                    .map_err(Into::into);
            }
        }
    }
}
