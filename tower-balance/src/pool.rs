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
use futures::{Async, Future, Poll};
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

    fn poll_pending_service(&mut self, mut fut: MS::Future) -> Poll<(), error::Error> {
        match fut.poll().map_err(Into::into)? {
            Async::NotReady => {
                self.pending_service = Some(fut);
                Ok(Async::NotReady)
            }
            Async::Ready(svc) => {
                let id = self.next_id;
                self.next_id += 1;
                self.cache.push_unready(id, svc);
                Ok(Async::Ready(()))
            }
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
    type Future = <MS::Service as Service<Req>>::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let adding = match self.pending_service.take() {
            None => false,
            Some(fut) => {
                self.poll_pending_service(fut)?;
                true
            }
        };

        // Drive pending services, including any just-added services, to ready.
        // If there are ready services, we're ready to dispatch a request
        // without doing any more work.
        self.cache.poll_unready();
        if self.cache.ready_len() > 0 {
            return Ok(Async::Ready(()));
        }

        if adding {
            // A service is already pending or was just added to the cache, so
            // defer until an unready service notifies availability.
            return Ok(Async::NotReady);
        }
        debug_assert!(self.pending_service.is_none());

        if self.cache.len() == self.max_services {
            // The cache is at capacity so we're stuck waiting until new an
            // existing service is available.
            return Ok(Async::NotReady);
        }

        let fut = self.make_service.make_service(());
        if self.poll_pending_service(fut)?.is_ready() {
            return Ok(self.cache.poll_unready());
        }

        Ok(Async::NotReady)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let n = self.cache.ready_len();
        debug_assert!(n > 0);
        self.cache.call_ready_index(n - 1, req)
    }
}
