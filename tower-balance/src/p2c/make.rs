use super::Balance;
use crate::error;
use futures_core::ready;
use pin_project::pin_project;
use rand::{rngs::SmallRng, SeedableRng};
use std::marker::PhantomData;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
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
#[pin_project]
#[derive(Debug)]
pub struct MakeFuture<F, Req> {
    #[pin]
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
    <<S::Response as Discover>::Service as Service<Req>>::Error: Into<error::Error>,
{
    type Response = Balance<S::Response, Req>;
    type Error = S::Error;
    type Future = MakeFuture<S::Future, Req>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, target: Target) -> Self::Future {
        MakeFuture {
            inner: self.inner.call(target),
            rng: self.rng.clone(),
            _marker: PhantomData,
        }
    }
}

impl<F, T, E, Req> Future for MakeFuture<F, Req>
where
    F: Future<Output = Result<T, E>>,
    T: Discover,
    <T as Discover>::Service: Service<Req>,
    <<T as Discover>::Service as Service<Req>>::Error: Into<error::Error>,
{
    type Output = Result<Balance<T, Req>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let inner = ready!(this.inner.poll(cx))?;
        let svc = Balance::new(inner, this.rng.clone());
        Poll::Ready(Ok(svc))
    }
}
