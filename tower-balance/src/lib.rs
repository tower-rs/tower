extern crate futures;
extern crate tower;
extern crate tower_discover;
extern crate ordermap;

use futures::{Future, Poll, Async};

use tower::Service;
use tower_discover::Discover;

use ordermap::OrderMap;

/// Balances requests across a set of inner services using a round-robin
/// strategy.
pub struct Balance<T>
where T: Discover,
{
    balancer: RoundRobin<T>,
}

/// Error produced by `Balance`
pub enum Error<T, U> {
    Inner(T),
    Balance(U),
    NotReady,
}

pub struct ResponseFuture<T>
where T: Discover,
{
    inner: <T::Service as Service>::Future,
}

/// Round-robin based load balancing
struct RoundRobin<T>
where T: Discover,
{
    /// The service discovery handle
    discover: T,

    /// The endpoints managed by the balancer
    endpoints: OrderMap<T::Key, T::Service>,

    /// Balancer entry to use when handling the next request
    pos: usize,
}

// ===== impl Balance =====

impl<T> Balance<T>
where T: Discover,
{
    /// Create a new balancer
    pub fn new(discover: T) -> Self {
        Balance {
            balancer: RoundRobin {
                discover,
                endpoints: OrderMap::new(),
                pos: 0,
            },
        }
    }
}

impl<T> Service for Balance<T>
where T: Discover,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = Error<T::Error, T::DiscoverError>;
    type Future = ResponseFuture<T>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.balancer.poll_ready()
            .map_err(Error::Balance)
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let response = self.balancer.next().call(request);
        ResponseFuture { inner: response }
    }
}

// ===== impl RoundRobin =====

impl<T> RoundRobin<T>
where T: Discover,
{
    // ===== potentially a trait API =====

    /// Returns `Ready` when the balancer is ready to accept a request.
    fn poll_ready(&mut self) -> Poll<(), T::DiscoverError> {
        try!(self.update_endpoints());

        let len = self.endpoints.len();

        // Loop over endpoints, finding the first that is available.
        for _ in 0..len {
            let res = self.endpoints
                .get_index_mut(self.pos)
                .unwrap().1
                .poll_ready();

            match res {
                Ok(Async::Ready(_)) => {
                    return Ok(Async::Ready(()));
                }

                // Go to the next one
                Ok(Async::NotReady) => {}

                // TODO: How to handle the error
                Err(_) => {}
            }

            self.inc_pos();
        }

        // Not ready
        Ok(Async::NotReady)
    }

    /// Returns a reference to the service to dispatch the next request to.
    ///
    /// It is expected that once `poll_ready` returns `Ready`, this function
    /// must succeed.
    ///
    /// # Panics
    ///
    /// This function may panic if `poll_ready` did not return `Ready`
    /// immediately prior.
    fn next(&mut self) -> &mut T::Service {
        let pos = self.pos;
        self.inc_pos();

        match self.endpoints.get_index_mut(pos) {
            Some((_, val)) => val,
            _ => panic!(),
        }
    }

    // ===== internal =====

    fn update_endpoints(&mut self) -> Result<(), T::DiscoverError> {
        use tower_discover::Change::*;

        loop {
            let change = match try!(self.discover.poll()) {
                Async::Ready(change) => change,
                Async::NotReady => return Ok(()),
            };

            match change {
                Insert(key, val) => {
                    // TODO: What to do if there already is an entry with the
                    // given `key` in the set?
                    self.endpoints.entry(key).or_insert(val);
                }
                Remove(key) => {
                    // TODO: What to do if there is no entry?
                    self.endpoints.remove(&key);
                }
            }

            // For now, we're just going to reset the pos to zero, but this is
            // definitely not ideal.
            //
            // TODO: improve
            self.pos = 0;
        }
    }

    fn inc_pos(&mut self) {
        self.pos = (self.pos + 1) % self.endpoints.len();
    }
}

// ===== impl ResponseFuture =====

impl<T> Future for ResponseFuture<T>
where T: Discover,
{
    type Item = T::Response;
    type Error = Error<T::Error, T::DiscoverError>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll()
            .map_err(Error::Inner)
    }
}
