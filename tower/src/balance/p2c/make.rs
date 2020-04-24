use super::Balance;
use crate::discover::Discover;
use futures_core::ready;
use pin_project::pin_project;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::hash::Hash;
use std::marker::PhantomData;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Construct load balancers over dynamic service sets produced by a wrapped "inner" service.
///
/// This is effectively an implementation of [`MakeService`](tower::make_service::MakeService),
/// except that it forwards the service descriptors (`Target`) to an inner service (`S`), and
/// expects that service to produce a service set in the form of a [`Discover`]. It then wraps the
/// service set in a [`Balance`] before returning it as the "made" service.
///
/// See the [module-level documentation](..) for details on load balancing.
#[derive(Clone, Debug)]
pub struct MakeBalance<S, Req> {
    inner: S,
    rng: SmallRng,
    _marker: PhantomData<fn(Req)>,
}

/// A [`Balance`] in the making.
#[pin_project]
#[derive(Debug)]
pub struct MakeFuture<F, Req> {
    #[pin]
    inner: F,
    rng: SmallRng,
    _marker: PhantomData<fn(Req)>,
}

impl<S, Req> MakeBalance<S, Req> {
    /// Build balancers using operating system entropy.
    pub fn new(make_discover: S) -> Self {
        Self {
            inner: make_discover,
            rng: SmallRng::from_entropy(),
            _marker: PhantomData,
        }
    }

    /// Build balancers using a seed from the provided random number generator.
    ///
    /// This may be preferrable when many balancers are initialized.
    pub fn from_rng<R: Rng>(inner: S, rng: R) -> Result<Self, rand::Error> {
        let rng = SmallRng::from_rng(rng)?;
        Ok(Self {
            inner,
            rng,
            _marker: PhantomData,
        })
    }
}

impl<S, Target, Req> Service<Target> for MakeBalance<S, Req>
where
    S: Service<Target>,
    S::Response: Discover,
    <S::Response as Discover>::Key: Hash,
    <S::Response as Discover>::Service: Service<Req>,
    <<S::Response as Discover>::Service as Service<Req>>::Error: Into<crate::BoxError>,
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
    <T as Discover>::Key: Hash,
    <T as Discover>::Service: Service<Req>,
    <<T as Discover>::Service as Service<Req>>::Error: Into<crate::BoxError>,
{
    type Output = Result<Balance<T, Req>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let inner = ready!(this.inner.poll(cx))?;
        let svc = Balance::from_rng(inner, this.rng.clone()).expect("SmallRng is infallible");
        Poll::Ready(Ok(svc))
    }
}
