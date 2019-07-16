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
    MS: MakeService<(), Request, Error = error::Error>,
    MS::MakeError: Into<error::Error>,
{
    next_id: usize,
    max_services: usize,
    make_service: MS,
    pending_service: Option<MS::Future>,
    cache: ReadyCache<usize, MS::Service, Request>,
}

impl<MS, Request> Pool<MS, Request>
where
    MS: MakeService<(), Request, Error = error::Error>,
    MS::Service: Load,
    <MS::Service as Load>::Metric: std::fmt::Debug,
    MS::MakeError: Into<error::Error>,
{
    /// Construct a new dynamically sized `Pool`.
    ///
    /// If many calls to `poll_ready` return `NotReady`, `new_service` is used to
    /// construct another `Service` that is then added to the load-balanced pool.
    /// If many calls to `poll_ready` succeed, the most recently added `Service`
    /// is dropped from the pool.
    pub fn new(max_services: usize, make_service: MS) -> Self {
        Self {
            make_service,
            max_services,
            next_id: 0,
            pending_service: None,
            cache: ReadyCache::default(),
        }
    }
}

impl<MS, Req> Service<Req> for Pool<MS, Req>
where
    MS: MakeService<(), Req, Error = error::Error>,
    MS::Service: Load,
    <MS::Service as Load>::Metric: std::fmt::Debug,
    MS::MakeError: Into<error::Error>,
{
    type Response = <MS::Service as Service<Req>>::Response;
    type Error = error::Error;
    type Future = future::MapErr<
        <MS::Service as Service<Req>>::Future,
        fn(<MS::Service as Service<Req>>::Error) -> error::Error,
    >;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if self.cache.poll_ready()?.is_ready() {
            return Ok(Async::Ready(()));
        }

        loop {
            if let Some(fut) = self.pending_service.as_mut() {
                let svc = try_ready!(fut.poll().map_err(Into::into));
                self.pending_service = None;

                let id = self.next_id;
                self.next_id += 1;
                self.cache.push_service(id, svc);

                return self.cache.poll_ready();
            }

            if self.cache.len() == self.max_services {
                return Ok(Async::NotReady);
            }

            self.pending_service = Some(self.make_service.make_service(()));
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        debug_assert!(self.cache.ready_len() > 0);
        self.cache.call_next_ready(req).map_err(Into::into)
    }
}
