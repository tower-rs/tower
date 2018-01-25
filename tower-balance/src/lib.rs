#[macro_use]
extern crate futures;
#[macro_use]
extern crate log;
extern crate ordermap;
#[cfg(test)]
extern crate quickcheck;
extern crate rand;
extern crate tower;
extern crate tower_discover;

use futures::{Future, Poll, Async};
use ordermap::OrderMap;
use rand::Rng;
use std::marker::PhantomData;
use tower::Service;
use tower_discover::Discover;

pub mod choose;
pub mod load;

pub use choose::Choose;
pub use load::Load;

/// Chooses services using the [Power of Two Choices][p2c].
///
/// This configuration is prefered when a load metric is known.
///
/// As described in the [Finagle Guide][finagle]:
/// > The algorithm randomly picks two services from the set of ready endpoints and selects
/// > the least loaded of the two. By repeatedly using this strategy, we can expect a
/// > manageable upper bound on the maximum load of any server.
/// >
/// > The maximum load variance between any two servers is bound by `ln(ln(n))` where `n`
/// > is the number of servers in the cluster.
///
/// [finagle]: https://twitter.github.io/finagle/guide/Clients.html#power-of-two-choices-p2c-least-loaded
/// [p2c]: http://www.eecs.harvard.edu/~michaelm/postscripts/handbook2001.pdf
pub fn power_of_two_choices<D, R>(loaded: D, rng: R) -> Balance<D, choose::PowerOfTwoChoices<R>>
where
    D: Discover,
    D::Service: Load,
    <D::Service as Load>::Metric: PartialOrd,
    R: Rng,
{
    Balance::new(loaded, choose::PowerOfTwoChoices::new(rng))
}

/// Attempts to choose services sequentially.
///
/// This configuration is prefered when no load metric is known.
pub fn round_robin<D: Discover>(discover: D) -> Balance<D, choose::RoundRobin> {
    Balance::new(discover, choose::RoundRobin::default())
}

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
    ready: OrderMap<D::Key, D::Service>,

    /// Newly-added endpoints that have not yet become ready.
    not_ready: OrderMap<D::Key, D::Service>,
}

/// Error produced by `Balance`
#[derive(Debug)]
pub enum Error<T, U> {
    Inner(T),
    Balance(U),
    NotReady,
}

pub struct ResponseFuture<F: Future, E>(F, PhantomData<E>);

// ===== impl Balance =====

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
            ready: OrderMap::default(),
            not_ready: OrderMap::default(),
        }
    }

    /// Polls `discover` for updates, adding new items to `not_ready`.
    ///
    /// Removals may alter the order of either `ready` or `not_ready`.
    fn update_from_discover(&mut self)
        -> Result<(), Error<<D::Service as Service>::Error, D::DiscoverError>>
    {
        debug!("updating from discover");
        use tower_discover::Change::*;

        while let Async::Ready(change) = self.discover.poll().map_err(Error::Balance)? {
            match change {
                Insert(key, mut svc) => {
                    if self.ready.contains_key(&key) || self.not_ready.contains_key(&key) {
                        // Ignore duplicate endpoints.
                        continue;
                    }

                    self.not_ready.insert(key, svc);
                }

                Remove(key) => {
                    let _ejected = match self.ready.remove(&key) {
                        None => self.not_ready.remove(&key),
                        Some(s) => Some(s),
                    };
                    // XXX is it safe to just drop the Service? Or do we need some sort of
                    // graceful teardown?
                }
            }
        }

        Ok(())
    }

    /// Calls `poll_ready` on all services in `not_ready`.
    ///
    /// When `poll_ready` returns ready, the service is removed from `not_ready` and inserted
    /// into `ready`, potentially altering the order of `ready` and/or `not_ready`.
    fn promote_to_ready(&mut self)
        -> Result<(), Error<<D::Service as Service>::Error, D::DiscoverError>>
    {
        let n = self.not_ready.len();
        debug!("promoting to ready: {}", n);
        // Iterate through the not-ready endpoints from right to left to prevent removals
        // from reordering services in a way that could prevent a service from being polled.
        for offset in 1..n {
            let idx = n - offset;

            let is_ready = {
                let (_, svc) = self.not_ready
                    .get_index_mut(idx)
                    .expect("invalid not_ready index");;
                svc.poll_ready().map_err(Error::Inner)?.is_ready()
            };

            if is_ready {
                debug!("promoting to ready");
                let (key, svc) = self.not_ready
                    .swap_remove_index(idx)
                    .expect("invalid not_ready index");
                self.ready.insert(key, svc);
            } else {
                debug!("not promoting to ready");
            }
        }

        Ok(())
    }

    /// Polls a `ready` service or moves it to `not_ready`.
    ///
    /// If the service exists in `ready` and does not poll as ready, it is moved to
    /// `not_ready`, potentially altering the order of `ready` and/or `not_ready`.
    fn poll_ready_index(&mut self, idx: usize)
        -> Option<Poll<(), Error<<D::Service as Service>::Error, D::DiscoverError>>>
    {
        match self.ready.get_index_mut(idx) {
            None => return None,
            Some((_, svc)) => {
                match svc.poll_ready() {
                    Ok(Async::Ready(())) => return Some(Ok(Async::Ready(()))),
                    Err(e) => return Some(Err(Error::Inner(e))),
                    Ok(Async::NotReady) => {}
                }
            }
        }

        let (key, svc) = self.ready.swap_remove_index(idx).expect("invalid ready index");
        self.not_ready.insert(key, svc);
        Some(Ok(Async::NotReady))
    }

    /// Chooses the next service to which a request will be dispatched.
    ///
    /// Ensures that .
    fn choose_and_poll_ready(&mut self)
        -> Poll<(), Error<<D::Service as Service>::Error, D::DiscoverError>>
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
            if self.poll_ready_index(idx).expect("invalid ready index")?.is_ready() {
                self.chosen_ready_index = Some(idx);
                return Ok(Async::Ready(()));
            }
        }
    }
}

impl<D, C> Service for Balance<D, C>
where
    D: Discover,
    C: Choose<D::Key, D::Service>,
{
    type Request = <D::Service as Service>::Request;
    type Response = <D::Service as Service>::Response;
    type Error = Error<<D::Service as Service>::Error, D::DiscoverError>;
    type Future = ResponseFuture<<D::Service as Service>::Future, D::DiscoverError>;

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
            self.poll_ready_index(idx).expect("invalid dispatched ready key")?;
        }

        // Update `not_ready` and `ready`.
        self.update_from_discover()?;
        self.promote_to_ready()?;

        // Choose the next service to be used by `call`.
        self.choose_and_poll_ready()
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let idx = self.chosen_ready_index.take().expect("not ready");
        let (_, svc) = self.ready.get_index_mut(idx).expect("invalid chosen ready index");
        self.dispatched_ready_index = Some(idx);

        let rsp = svc.call(request);
        ResponseFuture(rsp, PhantomData)
    }
}

// ===== impl ResponseFuture =====

impl<F: Future, E> Future for ResponseFuture<F, E> {
    type Item = F::Item;
    type Error = Error<F::Error, E>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll().map_err(Error::Inner)
    }
}
