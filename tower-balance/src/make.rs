use crate::P2CBalance;
use futures::{try_ready, Future, Poll};
use rand::{rngs::SmallRng, FromEntropy};
use tower_discover::Discover;
use tower_service::Service;

/// Makes `P2CBalancers` given an inner service that makes `Discover`s.
#[derive(Clone, Debug)]
pub struct MakeP2CBalance<S> {
    inner: S,
    rng: SmallRng,
}

/// Makes a balancer instance.
pub struct MakeFuture<F> {
    inner: F,
    rng: SmallRng,
}

impl<S> MakeP2CBalance<S> {
    pub(crate) fn new(inner: S, rng: SmallRng) -> Self {
        Self { inner, rng }
    }

    /// Initializes a P2C load balancer from the OS's entropy source.
    pub fn from_entropy(make_discover: S) -> Self {
        Self::new(make_discover, SmallRng::from_entropy())
    }
}

impl<S, Target> Service<Target> for MakeP2CBalance<S>
where
    S: Service<Target>,
    S::Response: Discover,
{
    type Response = P2CBalance<S::Response>;
    type Error = S::Error;
    type Future = MakeFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, target: Target) -> Self::Future {
        MakeFuture {
            inner: self.inner.call(target),
            rng: self.rng.clone(),
        }
    }
}

impl<F> Future for MakeFuture<F>
where
    F: Future,
    F::Item: Discover,
{
    type Item = P2CBalance<F::Item>;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let inner = try_ready!(self.inner.poll());
        let svc = P2CBalance::new(inner, self.rng.clone());
        Ok(svc.into())
    }
}
