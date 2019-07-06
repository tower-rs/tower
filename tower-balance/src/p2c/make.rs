use super::Balance;
use futures::{try_ready, Future, Poll};
use rand::{rngs::SmallRng, FromEntropy};
use std::marker::PhantomData;
use tower_discover::Discover;
use tower_service::Service;

/// Makes `Balancer`s given an inner service that makes `Discover`s.
#[derive(Clone, Debug)]
pub struct BalanceMake<S, Req> {
    inner: S,
    rng: SmallRng,
    _marker: PhantomData<fn(Req)>,
}

/// Makes a balancer instance.
pub struct MakeFuture<F, Req> {
    inner: F,
    rng: SmallRng,
    _marker: PhantomData<fn(Req)>,
}

impl<S, Req> BalanceMake<S, Req> {
    pub(crate) fn new(inner: S, rng: SmallRng) -> Self {
        Self {
            inner,
            rng,
            _marker: PhantomData,
        }
    }

    /// Initializes a P2C load balancer from the OS's entropy source.
    pub fn from_entropy(make_discover: S) -> Self {
        Self::new(make_discover, SmallRng::from_entropy())
    }
}

impl<S, Target, Req> Service<Target> for BalanceMake<S, Req>
where
    S: Service<Target>,
    S::Response: Discover,
    <S::Response as Discover>::Service: Service<Req>,
{
    type Response = Balance<S::Response, Req>;
    type Error = S::Error;
    type Future = MakeFuture<S::Future, Req>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, target: Target) -> Self::Future {
        MakeFuture {
            inner: self.inner.call(target),
            rng: self.rng.clone(),
            _marker: PhantomData,
        }
    }
}

impl<F, Req> Future for MakeFuture<F, Req>
where
    F: Future,
    F::Item: Discover,
    <F::Item as Discover>::Service: Service<Req>,
{
    type Item = Balance<F::Item, Req>;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let inner = try_ready!(self.inner.poll());
        let svc = Balance::new(inner, self.rng.clone());
        Ok(svc.into())
    }
}
