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

/// Creates a new Power of Two Choices load balancer.
pub fn power_of_two_choices<D, R>(discover: D, rng: R)
    -> Balance<D, choose::PowerOfTwoChoices<D::Key, D::Service, R>>
where
    D: Discover,
    D::Key: Clone,
    D::Service: Loaded,
    R: Rng,
{
    Balance::new(discover, choose::PowerOfTwoChoices::new(rng))
}

/// Creates a new Round Robin balancer.
///
/// The balancer selects each node in order. This order is not strictly enforced.
pub fn round_robin<D>(discover: D)
    -> Balance<D, choose::RoundRobin<D::Key, D::Service>>
where
    D: Discover,
    D::Service: Loaded,
    D::Key: Clone,
{
    Balance::new(discover, choose::RoundRobin::default())
}

/// Balances requests across a set of inner services.
pub struct Balance<D, C>
where
    D: Discover,
    D::Service: Loaded,
    C: Choose<Key = D::Key, Loaded = D::Service>,
{
    /// Provides endpoints from service discovery.
    discover: D,

    /// Determines which endpoint is ready to be used next.
    choose: C,

    /// Holds the key of an endpoint that has been selected to be used next.
    chosen_ready_index: Option<usize>,

    /// Holds the key of an endpoint after a request has been dispatched to it.
    ///
    /// This is so that `poll_ready` can check to see if this node should be moved to
    /// `not_ready`.
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
    C: Choose<Key = D::Key, Loaded = D::Service>,
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
    /// into `ready`.
    fn promote_not_ready(&mut self)
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
    /// `not_ready`.
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
    C: Choose<Key = D::Key, Loaded = D::Service>,
{
    type Request = <D::Service as Service>::Request;
    type Response = <D::Service as Service>::Response;
    type Error = Error<<D::Service as Service>::Error, D::DiscoverError>;
    type Future = ResponseFuture<<D::Service as Service>::Future, D::DiscoverError>;

    /// Prepares the balancer to process a request.
    ///
    /// When `Async::Ready` is returned, `chosen` has a value that refers to a
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // Check the readiness of a previously-chosen node, moving it to not-ready if
        // appropriate. This must be done before changing `ready`, since it can reorder
        // nodes.
        if let Some(idx) = self.dispatched_ready_index.take() {
            // XXX Should we swallow per-endpoint errors?
            self.poll_ready_index(idx).expect("invalid ready key")?;
        }

        // Updating from discovery adds new nodes to `not_ready` and may remove nodes from
        // either `ready` or `not_ready`; so, even though `chosen_ready_index` is
        // typically None, `poll_ready` may be called multuple times without `call` being
        // invoked, so we need to take care to clear out this state.
        self.chosen_ready_index = None;
        self.update_from_discover()?;
        self.promote_not_ready()?;

        // Choose a node from `ready` and ensure that it's still ready, since it's
        // possible that it was added some time ago. Continue doing this until a chosen
        // node is ready or there are no ready nodes.
        while !self.ready.is_empty() {
            let (idx, ..) = {
                let key = self.choose.call(&self.ready);
                self.ready.get_pair_index(key).expect("invalid ready key")
            };

            // XXX Should we swallow per-endpoint errors?
            if self.poll_ready_index(idx).expect("invalid ready index")?.is_ready() {
                self.chosen_ready_index = Some(idx);
                return Ok(Async::Ready(()));
            }
        }

        return Ok(Async::NotReady);
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let idx = self.chosen_ready_index.take().expect("not ready");
        let (_, svc) = self.ready.get_index_mut(idx).expect("invalid ready index");
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
