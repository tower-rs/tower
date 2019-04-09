#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]
#[cfg(test)]
extern crate quickcheck;

use futures::{Async, Poll};
use indexmap::IndexMap;
use log::{debug, trace};
use rand::{rngs::SmallRng, SeedableRng};
use std::fmt;
use tower_discover::Discover;
use tower_service::Service;

pub mod choose;
pub mod error;
pub mod future;
pub mod load;
pub mod pool;

#[cfg(test)]
mod test;

pub use self::{
    choose::Choose,
    load::{
        weight::{HasWeight, Weight, Weighted, WithWeighted},
        Load,
    },
    pool::Pool,
};

use self::{error::Error, future::ResponseFuture};

/// Balances requests across a set of inner services.
#[derive(Debug)]
pub struct Balance<D: Discover, C> {
    /// Provides endpoints from service discovery.
    discover: D,

    /// Determines which endpoint is ready to be used next.
    choose: C,

    /// Holds an index into `ready`, indicating the service that has been chosen to
    /// dispatch the next request.
    chosen_ready_index: Option<usize>,

    /// Holds an index into `ready`, indicating the service that dispatched the last
    /// request.
    dispatched_ready_index: Option<usize>,

    /// Holds all possibly-available endpoints (i.e. from `discover`).
    ready: IndexMap<D::Key, D::Service>,

    /// Newly-added endpoints that have not yet become ready.
    not_ready: IndexMap<D::Key, D::Service>,
}

// ===== impl Balance =====

impl<D> Balance<D, choose::PowerOfTwoChoices>
where
    D: Discover,
    D::Service: Load,
    <D::Service as Load>::Metric: PartialOrd + fmt::Debug,
{
    /// Chooses services using the [Power of Two Choices][p2c].
    ///
    /// This configuration is prefered when a load metric is known.
    ///
    /// As described in the [Finagle Guide][finagle]:
    ///
    /// > The algorithm randomly picks two services from the set of ready endpoints and
    /// > selects the least loaded of the two. By repeatedly using this strategy, we can
    /// > expect a manageable upper bound on the maximum load of any server.
    /// >
    /// > The maximum load variance between any two servers is bound by `ln(ln(n))` where
    /// > `n` is the number of servers in the cluster.
    ///
    /// [finagle]: https://twitter.github.io/finagle/guide/Clients.html#power-of-two-choices-p2c-least-loaded
    /// [p2c]: http://www.eecs.harvard.edu/~michaelm/postscripts/handbook2001.pdf
    pub fn p2c(discover: D) -> Self {
        Self::new(discover, choose::PowerOfTwoChoices::default())
    }

    /// Initializes a P2C load balancer from the provided randomization source.
    ///
    /// This may be preferable when an application instantiates many balancers.
    pub fn p2c_with_rng<R: rand::Rng>(discover: D, rng: &mut R) -> Result<Self, rand::Error> {
        let rng = SmallRng::from_rng(rng)?;
        Ok(Self::new(discover, choose::PowerOfTwoChoices::new(rng)))
    }
}

impl<D: Discover> Balance<D, choose::RoundRobin> {
    /// Attempts to choose services sequentially.
    ///
    /// This configuration is prefered when no load metric is known.
    pub fn round_robin(discover: D) -> Self {
        Self::new(discover, choose::RoundRobin::default())
    }
}

impl<D, C> Balance<D, C>
where
    D: Discover,
    C: Choose<D::Key, D::Service>,
{
    /// Creates a new balancer.
    pub fn new(discover: D, choose: C) -> Self {
        Self {
            discover,
            choose,
            chosen_ready_index: None,
            dispatched_ready_index: None,
            ready: IndexMap::default(),
            not_ready: IndexMap::default(),
        }
    }

    /// Returns true iff there are ready services.
    ///
    /// This is not authoritative and is only useful after `poll_ready` has been called.
    pub fn is_ready(&self) -> bool {
        !self.ready.is_empty()
    }

    /// Returns true iff there are no ready services.
    ///
    /// This is not authoritative and is only useful after `poll_ready` has been called.
    pub fn is_not_ready(&self) -> bool {
        self.ready.is_empty()
    }

    /// Counts the number of services considered to be ready.
    ///
    /// This is not authoritative and is only useful after `poll_ready` has been called.
    pub fn num_ready(&self) -> usize {
        self.ready.len()
    }

    /// Counts the number of services not considered to be ready.
    ///
    /// This is not authoritative and is only useful after `poll_ready` has been called.
    pub fn num_not_ready(&self) -> usize {
        self.not_ready.len()
    }
}

impl<D, C> Balance<D, C>
where
    D: Discover,
    D::Error: Into<Error>,
    C: Choose<D::Key, D::Service>,
{
    /// Polls `discover` for updates, adding new items to `not_ready`.
    ///
    /// Removals may alter the order of either `ready` or `not_ready`.
    fn update_from_discover(&mut self) -> Result<(), error::Balance> {
        debug!("updating from discover");
        use tower_discover::Change::*;

        while let Async::Ready(change) =
            self.discover.poll().map_err(|e| error::Balance(e.into()))?
        {
            match change {
                Insert(key, svc) => {
                    // If the `Insert`ed service is a duplicate of a service already
                    // in the ready list, remove the ready service first. The new
                    // service will then be inserted into the not-ready list.
                    self.ready.remove(&key);

                    self.not_ready.insert(key, svc);
                }

                Remove(key) => {
                    let _ejected = match self.ready.remove(&key) {
                        None => self.not_ready.remove(&key),
                        Some(s) => Some(s),
                    };
                    // XXX is it safe to just drop the Service? Or do we need some sort of
                    // graceful teardown?
                    // TODO: poll_close
                }
            }
        }

        Ok(())
    }

    /// Calls `poll_ready` on all services in `not_ready`.
    ///
    /// When `poll_ready` returns ready, the service is removed from `not_ready` and inserted
    /// into `ready`, potentially altering the order of `ready` and/or `not_ready`.
    fn promote_to_ready<Request>(&mut self) -> Result<(), <D::Service as Service<Request>>::Error>
    where
        D::Service: Service<Request>,
    {
        let n = self.not_ready.len();
        if n == 0 {
            trace!("promoting to ready: not_ready is empty, skipping.");
            return Ok(());
        }

        debug!("promoting to ready: {}", n);
        // Iterate through the not-ready endpoints from right to left to prevent removals
        // from reordering services in a way that could prevent a service from being polled.
        for idx in (0..n).rev() {
            let is_ready = {
                let (_, svc) = self
                    .not_ready
                    .get_index_mut(idx)
                    .expect("invalid not_ready index");;
                svc.poll_ready()?.is_ready()
            };
            trace!("not_ready[{:?}]: is_ready={:?};", idx, is_ready);
            if is_ready {
                debug!("not_ready[{:?}]: promoting to ready", idx);
                let (key, svc) = self
                    .not_ready
                    .swap_remove_index(idx)
                    .expect("invalid not_ready index");
                self.ready.insert(key, svc);
            } else {
                debug!("not_ready[{:?}]: not promoting to ready", idx);
            }
        }

        debug!("promoting to ready: done");

        Ok(())
    }

    /// Polls a `ready` service or moves it to `not_ready`.
    ///
    /// If the service exists in `ready` and does not poll as ready, it is moved to
    /// `not_ready`, potentially altering the order of `ready` and/or `not_ready`.
    fn poll_ready_index<Request>(
        &mut self,
        idx: usize,
    ) -> Option<Poll<(), <D::Service as Service<Request>>::Error>>
    where
        D::Service: Service<Request>,
    {
        match self.ready.get_index_mut(idx) {
            None => return None,
            Some((_, svc)) => match svc.poll_ready() {
                Ok(Async::Ready(())) => return Some(Ok(Async::Ready(()))),
                Err(e) => return Some(Err(e)),
                Ok(Async::NotReady) => {}
            },
        }

        let (key, svc) = self
            .ready
            .swap_remove_index(idx)
            .expect("invalid ready index");
        self.not_ready.insert(key, svc);
        Some(Ok(Async::NotReady))
    }

    /// Chooses the next service to which a request will be dispatched.
    ///
    /// Ensures that .
    fn choose_and_poll_ready<Request>(
        &mut self,
    ) -> Poll<(), <D::Service as Service<Request>>::Error>
    where
        D::Service: Service<Request>,
    {
        loop {
            let n = self.ready.len();
            debug!("choosing from {} replicas", n);
            let idx = match n {
                0 => return Ok(Async::NotReady),
                1 => 0,
                _ => {
                    let replicas = choose::replicas(&self.ready).expect("too few replicas");
                    self.choose.choose(replicas)
                }
            };

            // XXX Should we handle per-endpoint errors?
            if self
                .poll_ready_index(idx)
                .expect("invalid ready index")?
                .is_ready()
            {
                self.chosen_ready_index = Some(idx);
                return Ok(Async::Ready(()));
            }
        }
    }
}

impl<D, C, Svc, Request> Service<Request> for Balance<D, C>
where
    D: Discover<Service = Svc>,
    D::Error: Into<Error>,
    Svc: Service<Request>,
    Svc::Error: Into<Error>,
    C: Choose<D::Key, Svc>,
{
    type Response = Svc::Response;
    type Error = Error;
    type Future = ResponseFuture<Svc::Future>;

    /// Prepares the balancer to process a request.
    ///
    /// When `Async::Ready` is returned, `chosen_ready_index` is set with a valid index
    /// into `ready` referring to a `Service` that is ready to disptach a request.
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // Clear before `ready` is altered.
        self.chosen_ready_index = None;

        // Before `ready` is altered, check the readiness of the last-used service, moving it
        // to `not_ready` if appropriate.
        if let Some(idx) = self.dispatched_ready_index.take() {
            // XXX Should we handle per-endpoint errors?
            self.poll_ready_index(idx)
                .expect("invalid dispatched ready key")
                .map_err(Into::into)?;
        }

        // Update `not_ready` and `ready`.
        self.update_from_discover()?;
        self.promote_to_ready().map_err(Into::into)?;

        // Choose the next service to be used by `call`.
        self.choose_and_poll_ready().map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let idx = self.chosen_ready_index.take().expect("not ready");
        let (_, svc) = self
            .ready
            .get_index_mut(idx)
            .expect("invalid chosen ready index");
        self.dispatched_ready_index = Some(idx);

        let rsp = svc.call(request);
        ResponseFuture::new(rsp)
    }
}
