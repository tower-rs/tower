extern crate futures;
extern crate tower;
extern crate tower_discover;

use futures::{Future, Poll};

use tower::Service;
use tower_discover::Discover;

/// Balances requests across a set of inner services using a round-robin
/// strategy.
pub struct Balance<T> {
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
struct RoundRobin<T> {
    discover: T,
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
            },
        }
    }
}

impl<T> Service for Balance<T>
where T: Discover,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = Error<T::ServiceError, T::Error>;
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
    /// Returns `Ready` when the balancer is ready to accept a request.
    fn poll_ready(&mut self) -> Poll<(), T::Error> {
        unimplemented!();
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
        unimplemented!();
    }
}

// ===== impl ResponseFuture =====

impl<T> Future for ResponseFuture<T>
where T: Discover,
{
    type Item = T::Response;
    type Error = Error<T::ServiceError, T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        unimplemented!();
    }
}
