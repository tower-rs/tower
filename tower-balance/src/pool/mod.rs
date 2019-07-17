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

mod make_ready;
// #[cfg(test)]
// mod test;

use self::make_ready::MakeReady;
use crate::error;
use futures::{future, Async, Poll};
use tower_load::Load;
use tower_ready_cache::ReadyCache;
use tower_service::Service;
use tower_util::MakeService;

/// A dynamically sized pool of `Service` instances.
pub struct Pool<MS, Req>
where
    MS: MakeService<(), Req>,
    MS::Error: Into<error::Error>,
    MS::MakeError: Into<error::Error>,
{
    next_id: usize,
    min_services: usize,
    max_services: usize,
    make_service: MS,
    cache: ReadyCache<usize, MakeReady<MS, (), Req>, Req>,
}

impl<MS, Req> Pool<MS, Req>
where
    MS: MakeService<(), Req>,
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
        self.cache.push_service(id, MakeReady::from_future(fut));
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
        while self.cache.len() < self.min_services {
            self.push_new_service();
        }

        loop {
            while self.cache.poll_pending().is_err() {
                // If a service was dropped from the cache, backfill any missing
                // services it may need.
                while self.cache.len() < self.min_services {
                    self.push_new_service();
                }
            }

            // As long as there are ready nodes, try to use one of them.
            while self.cache.ready_len() > 0 {
                if let Ok(Async::Ready(())) = self.cache.poll_next_ready_index(0) {
                    return Ok(Async::Ready(()));
                }
            }

            debug_assert!(self.cache.ready_len() == 0);
            if self.cache.pending_len() == self.max_services {
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
