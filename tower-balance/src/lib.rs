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

pub mod load;
pub mod select;

pub use load::{Load, Loaded};
pub use select::Select;

/// Creates a new Power of Two Choices load balancer.
pub fn power_of_two_choices<D, R>(discover: D, rng: R)
    -> Balance<D, select::PowerOfTwoChoices<D::Key, D::Service, R>>
where
    D: Discover,
    D::Key: Clone,
    D::Service: Loaded,
    R: Rng,
{
    Balance::new(discover, select::PowerOfTwoChoices::new(rng))
}

/// Creates a new Round Robin balancer.
///
/// The balancer selects each node in order. This order is not strictly enforced.
pub fn round_robin<D>(discover: D)
    -> Balance<D, select::RoundRobin<D::Key, D::Service>>
where
    D: Discover,
    D::Service: Loaded,
    D::Key: Clone,
{
    Balance::new(discover, select::RoundRobin::default())
}

/// Balances requests across a set of inner services.
pub struct Balance<D, S>
where
    D: Discover,
    D::Key: Clone,
    D::Service: Loaded,
    S: Select<Key = D::Key, Loaded = D::Service>,
{
    /// Provides endpoints from service discovery.
    discover: D,

    /// Determines which endpoint is ready to be used next.
    select: S,

    /// Newly-added endpoints that have not yet become ready.
    not_ready: OrderMap<D::Key, D::Service>,

    /// Holds all possibly-available endpoints (i.e. from `discover`).
    ready: OrderMap<D::Key, D::Service>,

    /// Holds the key of an endpoint that has been selected to be used next.
    selected: Option<D::Key>,
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

impl<D, S> Balance<D, S>
where
    D: Discover,
    D::Key: Clone,
    D::Service: Loaded,
    S: Select<Key = D::Key, Loaded = D::Service>,
{
    /// Creates a new balancer.
    pub fn new(discover: D, select: S) -> Self {
        Self {
            discover,
            select,
            not_ready: OrderMap::default(),
            ready: OrderMap::default(),
            selected: None,
        }
    }

    fn update_from_discover(&mut self) -> Result<(), Error<<D::Service as Service>::Error, D::DiscoverError>> {
        use tower_discover::Change::*;

        while let Async::Ready(change) = self.discover.poll().map_err(Error::Balance)? {
            match change {
                Insert(key, mut svc) => {
                    if self.ready.contains_key(&key) || self.not_ready.contains_key(&key) {
                        // Ignore duplicate endpoints.
                        continue;
                    }

                    if svc.poll_ready().map_err(Error::Inner)?.is_ready() {
                        self.ready.insert(key, svc);
                    } else {
                        self.not_ready.insert(key, svc);
                    }
                }

                Remove(key) => {
                    let _ejected = match self.ready.remove(&key) {
                        None => self.not_ready.remove(&key),
                        Some(s) => Some(s),
                    };
                    // TODO we need some sort of graceful shutdown here.
                }
            }
        }

        Ok(())
    }

    /// Polls unready nodes for readiness.
    ///
    /// Returns ready iff there is at least one ready node.
    fn poll_not_ready(&mut self) -> Poll<(), Error<<D::Service as Service>::Error, D::DiscoverError>> {
        // Poll all  not-ready endpoints to ensure they
        // Iterate through the not-ready endpoints from right to left to prevent removals
        // from reordering nodes in a way that could prevent a node from being polled.
        for idx in self.not_ready.len()-1..0 {
            let poll = {
                let (_, mut svc) = self.not_ready.get_index_mut(idx).unwrap();
                svc.poll_ready().map_err(Error::Inner)?
            };

            if poll.is_ready() {
                let (key, svc) = self.not_ready.swap_remove_index(idx).unwrap();
                self.ready.insert(key, svc);
            }
        }

        if self.ready.is_empty() {
            Ok(Async::NotReady)
        } else {
            Ok(Async::Ready(()))
        }
    }
}

impl<D, S> Service for Balance<D, S>
where
    D: Discover,
    D::Key: Clone,
    D::Service: Loaded,
    S: Select<Key = D::Key, Loaded = D::Service>,
{
    type Request = <D::Service as Service>::Request;
    type Response = <D::Service as Service>::Response;
    type Error = Error<<D::Service as Service>::Error, D::DiscoverError>;
    type Future = ResponseFuture<<D::Service as Service>::Future, D::DiscoverError>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.update_from_discover()?;
        try_ready!(self.poll_not_ready());
        debug_assert!(!self.ready.is_empty(), "Balance is ready when there are endpoints");

        let key = self.select.call(&self.ready);
        self.selected = Some(key.clone());

        Ok(Async::Ready(()))
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let key = self.selected.take().expect("not ready");
        let svc = self.ready.get_mut(&key).expect("not ready");

        ResponseFuture(svc.call(request), PhantomData)
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
