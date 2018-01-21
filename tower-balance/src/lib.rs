#[macro_use]
extern crate futures;
extern crate ordermap;
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
pub use load::{Load, Loaded};

/// Chooses nodes using the [Power of Two Choices][p2c].
///
/// This configuration is prefered when a load metric is known.
///
/// As described in the [Finagle Guide][finagle]:
/// > The algorithm randomly picks two nodes from the set of ready endpoints and selects
/// > the least loaded of the two. By repeatedly using this strategy, we can expect a
/// > manageable upper bound on the maximum load of any server.
/// >
/// > The maximum load variance between any two servers is bound by `ln(ln(n))` where `n`
/// > is the number of servers in the cluster.
///
/// [finagle]: https://twitter.github.io/finagle/guide/Clients.html#power-of-two-choices-p2c-least-loaded
/// [p2c]: http://www.eecs.harvard.edu/~michaelm/postscripts/handbook2001.pdf
pub fn power_of_two_choices<D, R>(discover: D, rng: R) -> PowerOfTwoChoices<D, R>
where
    D: Discover,
    D::Service: Loaded,
    R: Rng,
{
    Balance::new(discover, choose::PowerOfTwoChoices::new(rng))
}

/// Attempts to choose nodes sequentially.
///
/// This configuration is prefered no load metric is known.
pub fn round_robin<D: Discover>(discover: D) -> RoundRobin<D> {
    Balance::new(load::Constant::new(discover, Load::MAX), choose::RoundRobin::default())
}

pub type PowerOfTwoChoices<D, R> = Balance<D, choose::PowerOfTwoChoices<R>>;

pub type RoundRobin<D> = Balance<load::Constant<D>, choose::RoundRobin>;

/// Balances requests across a set of inner services.
#[derive(Debug)]
pub struct Balance<D: Discover, C> {
    /// Provides endpoints from service discovery.
    discover: D,

    /// Determines which endpoint is ready to be used next.
    choose: C,

    /// Holds an index into `ready`, indicating the node that has been chosen to dispatch
    /// the next request.
    chosen_ready_index: Option<usize>,

    /// Holds an index into `ready`, indicating the node that dispatched the last request.
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
    D::Service: Loaded,
    C: Choose,
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
    /// When `poll_ready` returns ready, the node is removed from `not_ready` and inserted
    /// into `ready`, potentially altering the order of `ready` and/or `not_ready`.
    fn promote_to_ready(&mut self)
        -> Result<(), Error<<D::Service as Service>::Error, D::DiscoverError>>
    {
        // Iterate through the not-ready endpoints from right to left to prevent removals
        // from reordering nodes in a way that could prevent a node from being polled.
        for idx in self.not_ready.len()-1..0 {
            let is_ready = {
                let (_, svc) = self.not_ready
                    .get_index_mut(idx)
                    .expect("invalid not_ready index");;
                svc.poll_ready().map_err(Error::Inner)?.is_ready()
            };

            if is_ready {
                let (key, svc) = self.not_ready
                    .swap_remove_index(idx)
                    .expect("invalid not_ready index");
                self.ready.insert(key, svc);
            }
        }

        Ok(())
    }

    /// If the key exists in `ready`, poll its readiness.
    ///
    /// If the node exists in `ready` and does not poll as ready, it is moved to
    /// `not_ready`, potentially altering the order of `ready` and/or `not_ready`.
    pub fn poll_ready_index(&mut self, idx: usize)
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
}

impl<D, C> Service for Balance<D, C>
where
    D: Discover,
    D::Service: Loaded,
    C: Choose,
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
        // Before `ready` is altered, clear out any chosen index.
        self.chosen_ready_index = None;

        // Before `ready` is altered, check the readiness of the last-used node, moving it
        // to `not_ready` if appropriate.
        if let Some(idx) = self.dispatched_ready_index.take() {
            // XXX Should we handle per-endpoint errors?
            self.poll_ready_index(idx).expect("invalid dispatched ready key")?;
        }

        // Update `ready` and `not_ready`.
        self.update_from_discover()?;
        self.promote_to_ready()?;

        // Choose a node from `ready` and ensure that it's still ready, since it's
        // possible that it was added some time ago. Continue doing this until a chosen
        // node is ready or there are no ready nodes.
        loop {
            let idx = match self.ready.len() {
                0 => return Ok(Async::NotReady),
                1 => 0,
                _ => self.choose.call(&self.ready),
            };

            // XXX Should we handle per-endpoint errors?
            if self.poll_ready_index(idx).expect("invalid ready index")?.is_ready() {
                self.chosen_ready_index = Some(idx);
                return Ok(Async::Ready(()));
            }
        }
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
