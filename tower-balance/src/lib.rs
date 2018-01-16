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

pub use load::{Load, PollLoad, WithLoad};
pub use select::Select;

/// Balances requests across a set of inner services.
pub struct Balance<D, L, S>
where
    D: Discover,
    D::Key: Clone,
    L: WithLoad<Service = D::Service>,
    S: Select<Key = D::Key, Service = L::PollLoad>,
{
    /// Provides endpoints from service discovery.
    discover: D,

    /// Wraps endpoint services with a load metric.
    with_load: L,

    /// Determines which endpoint is ready to be used next.
    select: S,

    /// Holds the key of an endpoint that has been selected to be used next.
    ready_endpoint: Option<D::Key>,

    /// Holds all possibly-available endpoints (i.e. from `discover`).
    endpoints: OrderMap<D::Key, L::PollLoad>,

    /// Newly-added endpoints that have not yet become ready.
    new_endpoints: OrderMap<D::Key, L::PollLoad>,
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

impl<D, L, S> Balance<D, L, S>
where
    D: Discover,
    D::Key: Clone,
    L: WithLoad<Service = D::Service>,
    S: Select<Key = D::Key, Service = L::PollLoad>,
{
    /// Creates a new balancer.
    pub fn new(discover: D, with_load: L, select: S) -> Self {
        Self {
            discover,
            select,
            with_load,
            ready_endpoint: None,
            endpoints: OrderMap::default(),
            new_endpoints: OrderMap::default(),
        }
    }

    /// Polls discovery for updates.
    ///
    /// Returns ready iff there is at least ones endpoint.
    fn poll_discover(&mut self) -> Poll<(), Error<<L::PollLoad as Service>::Error, D::DiscoverError>> {
        use tower_discover::Change::*;

        for idx in self.new_endpoints.len()-1..0 {
            let poll = {
                let (_, mut svc) = self.new_endpoints.get_index_mut(idx).unwrap();
                svc.poll_ready().map_err(Error::Inner)?
            };

            if poll.is_ready() {
                let (key, svc) = self.new_endpoints.swap_remove_index(idx).unwrap();
                self.endpoints.insert(key, svc);
            }
        }

        while let Async::Ready(change) = try!(self.discover.poll().map_err(Error::Balance)) {
            match change {
                Insert(key, svc) => {
                    if self.endpoints.contains_key(&key) || self.new_endpoints.contains_key(&key) {
                        // Ignore duplicate endpoints.
                        continue;
                    }

                    let mut svc = self.with_load.with_load(svc);

                    let poll = svc.poll_load().map_err(Error::Inner)?;
                    if poll.is_ready() {
                        self.endpoints.insert(key, svc);
                    } else {
                        self.new_endpoints.insert(key, svc);
                    }
                }

                Remove(key) => {
                    self.endpoints.remove(&key);
                    self.new_endpoints.remove(&key);
                }
            }
        }

        if self.endpoints.is_empty() {
            Ok(Async::NotReady)
        } else {
            Ok(Async::Ready(()))
        }
    }
}

impl<D> Balance<
    D,
    load::WithConstant<D::Service>,
    select::RoundRobin<D::Key, load::Constant<D::Service>>,
>
where
    D: Discover,
    D::Key: Clone,
{
    /// Creates a new balancer.
    ///
    /// The balancer selects each node in order. This order is not strictly enforced.
    pub fn round_robin(discover: D) -> Self {
        let load = load::WithConstant::min();
        let select = select::RoundRobin::default();
        Self::new(discover, load, select)
    }
}

impl<D, R> Balance<
    D,
    load::WithPendingRequests<D::Service>,
    select::PowerOfTwoChoices<D::Key, load::PendingRequests<D::Service>, R>,
>
where
    D: Discover,
    D::Key: Clone,
    R: Rng,
{
    /// Creates a new balancer.
    ///
    /// The balancer selects the least-loaded endpoint using Mitzenmacher's _Power of Two
    /// Choices_.
    pub fn pending_requests_p2c(discover: D, rng: R) -> Self {
        let load = load::WithPendingRequests::default();
        let select = select::PowerOfTwoChoices::new(rng);
        Self::new(discover, load, select)
    }
}

impl<D, L, S> Service for Balance<D, L, S>
where
    D: Discover,
    D::Key: Clone,
    L: WithLoad<Service = D::Service>,
    S: Select<Key = D::Key, Service = L::PollLoad>,
{
    type Request = <L::PollLoad as Service>::Request;
    type Response = <L::PollLoad as Service>::Response;
    type Error = Error<<L::PollLoad as Service>::Error, D::DiscoverError>;
    type Future = ResponseFuture<<L::PollLoad as Service>::Future, D::DiscoverError>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        try_ready!(self.poll_discover());
        assert!(!self.endpoints.is_empty(), "Balance is ready when there are endpoints");

        let nodes = self.endpoints.iter_mut();
        let key =  try_ready!(self.select.poll_next_ready(nodes).map_err(Error::Inner));
        self.ready_endpoint = Some(key.clone());

        Ok(Async::Ready(()))
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let key = self.ready_endpoint.take().expect("not ready");
        let svc = self.endpoints.get_mut(&key).expect("not ready");

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
