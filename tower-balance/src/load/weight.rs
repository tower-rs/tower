use futures::{try_ready, Async, Poll};
use std::ops;
use tower_discover::{Change, Discover};
use tower_service::Service;

use crate::Load;

/// A weight on [0.0, ∞].
///
/// Lesser-weighted nodes receive less traffic than heavier-weighted nodes.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Weight(u32);

/// A Service, that implements Load, that
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Weighted<T> {
    inner: T,
    weight: Weight,
}

#[derive(Debug)]
pub struct WithWeighted<T>(T);

pub trait HasWeight {
    fn weight(&self) -> Weight;
}

// === impl Weighted ===

impl<T: HasWeight> From<T> for Weighted<T> {
    fn from(inner: T) -> Self {
        let weight = inner.weight();
        Self { inner, weight }
    }
}

impl<T> HasWeight for Weighted<T> {
    fn weight(&self) -> Weight {
        self.weight
    }
}

impl<T> Weighted<T> {
    pub fn new<W: Into<Weight>>(inner: T, w: W) -> Self {
        let weight = w.into();
        Self { inner, weight }
    }

    pub fn into_parts(self) -> (T, Weight) {
        let Self { inner, weight } = self;
        (inner, weight)
    }
}

impl<L> Load for Weighted<L>
where
    L: Load,
    L::Metric: ops::Div<Weight>,
    <L::Metric as ops::Div<Weight>>::Output: PartialOrd,
{
    type Metric = <L::Metric as ops::Div<Weight>>::Output;

    fn load(&self) -> Self::Metric {
        self.inner.load() / self.weight
    }
}

impl<R, S: Service<R>> Service<R> for Weighted<S> {
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: R) -> Self::Future {
        self.inner.call(req)
    }
}

// === impl WithWeighted ===

impl<D> From<D> for WithWeighted<D>
where
    D: Discover,
    D::Key: HasWeight,
{
    fn from(d: D) -> Self {
        WithWeighted(d)
    }
}

impl<D> Discover for WithWeighted<D>
where
    D: Discover,
    D::Key: HasWeight,
{
    type Key = D::Key;
    type Error = D::Error;
    type Service = Weighted<D::Service>;

    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, Self::Error> {
        let c = match try_ready!(self.0.poll()) {
            Change::Remove(k) => Change::Remove(k),
            Change::Insert(k, svc) => {
                let w = k.weight();
                Change::Insert(k, Weighted::new(svc, w))
            }
        };

        Ok(Async::Ready(c))
    }
}

// === impl Weight ===

impl Weight {
    pub const ZERO: Weight = Weight(0);
    pub const UNIT: Weight = Weight(10_000);
    pub const MAX: Weight = Weight(std::u32::MAX);
}

impl Default for Weight {
    fn default() -> Self {
        Weight::UNIT
    }
}

impl From<f64> for Weight {
    fn from(w: f64) -> Self {
        if w < 0.0 || w == std::f64::NAN {
            Self::ZERO
        } else if w == std::f64::INFINITY {
            Self::MAX
        } else {
            Weight((w * 10_000.0).round() as u32)
        }
    }
}

impl Into<f64> for Weight {
    fn into(self) -> f64 {
        let v: f64 = self.0.into();
        v / 10_000.0
    }
}

impl ops::Div<Weight> for f64 {
    type Output = f64;

    fn div(self, w: Weight) -> f64 {
        if w == Weight::ZERO {
            ::std::f64::INFINITY
        } else {
            let w: f64 = w.into();
            self / w
        }
    }
}

impl ops::Div<Weight> for usize {
    type Output = f64;

    fn div(self, w: Weight) -> f64 {
        self as f64 / w
    }
}

#[test]
fn div_min() {
    assert_eq!(10.0 / Weight::ZERO, ::std::f64::INFINITY);
    assert_eq!(10 / Weight::ZERO, ::std::f64::INFINITY);
    assert_eq!(0 / Weight::ZERO, ::std::f64::INFINITY);
}
