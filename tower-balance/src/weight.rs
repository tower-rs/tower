use futures::{Async, Poll};
use std::ops;
use tower::Service;
use tower_discover::{Change, Discover};

use Load;

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct Weight(f64);

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Weighted<T> {
    inner: T,
    weight: Weight
}

// ===== impl Weighted =====

impl<T> Weighted<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            weight: Weight::DEFAULT,
        }
    }

    pub fn weight(&self) -> Weight {
        self.weight
    }

    pub fn weighted(self, weight: Weight) -> Self {
        Self {
            weight: weight,
            .. self
        }
    }

    pub fn map<F, U>(self, map: F) -> Weighted<U>
    where
        F: FnOnce(T) -> U
    {
        Weighted {
            inner: map(self.inner),
            weight: self.weight,
        }
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

impl<S: Service> Service for Weighted<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        self.inner.call(req)
    }
}

impl<D: Discover> Discover for Weighted<D> {
    type Key = D::Key;
    type Request = D::Request;
    type Response = D::Response;
    type Error = D::Error;
    type Service = Weighted<D::Service>;
    type DiscoverError = D::DiscoverError;

    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::DiscoverError> {
        use self::Change::*;

        let change = match try_ready!(self.inner.poll()) {
            Insert(k, svc) => Insert(k, Weighted::new(svc).weighted(self.weight)),
            Remove(k) => Remove(k),
        };

        Ok(Async::Ready(change))
    }
}

// ===== impl Weight =====

impl Weight {
    pub const MIN: Weight = Weight(0.0);
    pub const DEFAULT: Weight = Weight(1.0);
}

impl Default for Weight {
    fn default() -> Self {
        Weight::DEFAULT
    }
}

impl From<f64> for Weight {
    fn from(w: f64) -> Self {
        if w < 0.0 {
            Weight::MIN
        } else {
            Weight(w)
        }
    }
}

impl Into<f64> for Weight {
    fn into(self) -> f64 {
        self.0
    }
}

impl ops::Div<Weight> for f64 {
    type Output = f64;

    fn div(self, w: Weight) -> f64 {
        if w.0 == 0.0 {
            ::std::f64::INFINITY
        } else {
            self / w.0
        }
    }
}

impl ops::Div<Weight> for usize {
    type Output = f64;

    fn div(self, w: Weight) -> f64 {
        (self as f64) / w
    }
}

#[test]
fn div_min() {
    assert_eq!(10.0 / Weight::MIN, ::std::f64::INFINITY);
    assert_eq!(10 / Weight::MIN, ::std::f64::INFINITY);
    assert_eq!(0 / Weight::MIN, ::std::f64::INFINITY);
}
