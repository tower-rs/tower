use futures::{try_ready, Async, Poll};
use std::{ops, sync::Arc};
use tower_discover::{Change, Discover};
use tower_service::Service;

use super::{Instrument, InstrumentFuture, NoInstrument};
use crate::{HasWeight, Load, Weight};

/// Expresses load based on the number of currently-pending requests.
#[derive(Debug)]
pub struct PendingRequests<S, I = NoInstrument> {
    service: S,
    ref_count: RefCount,
    instrument: I,
}

/// Shared between instances of `PendingRequests` and `Handle` to track active
/// references.
#[derive(Clone, Debug, Default)]
struct RefCount(Arc<()>);

/// Wraps `inner`'s services with `PendingRequests`.
#[derive(Debug)]
pub struct WithPendingRequests<D, I = NoInstrument> {
    discover: D,
    instrument: I,
}

/// Represents the number of currently-pending requests to a given service.
#[derive(Clone, Copy, Debug, Default, PartialOrd, PartialEq, Ord, Eq)]
pub struct Count(usize);

#[derive(Debug)]
pub struct Handle(RefCount);

// ===== impl Count =====

impl ops::Div<Weight> for Count {
    type Output = f64;

    fn div(self, weight: Weight) -> f64 {
        self.0 / weight
    }
}

// ===== impl PendingRequests =====

impl<S, I> PendingRequests<S, I> {
    fn new(service: S, instrument: I) -> Self {
        Self {
            service,
            instrument,
            ref_count: RefCount::default(),
        }
    }

    fn handle(&self) -> Handle {
        Handle(self.ref_count.clone())
    }
}

impl<S, I> Load for PendingRequests<S, I> {
    type Metric = Count;

    fn load(&self) -> Count {
        // Count the number of references that aren't `self`.
        Count(self.ref_count.ref_count() - 1)
    }
}

impl<S: HasWeight, I> HasWeight for PendingRequests<S, I> {
    fn weight(&self) -> Weight {
        self.service.weight()
    }
}

impl<S, I, Request> Service<Request> for PendingRequests<S, I>
where
    S: Service<Request>,
    I: Instrument<Handle, S::Response>,
{
    type Response = I::Output;
    type Error = S::Error;
    type Future = InstrumentFuture<S::Future, I, Handle>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        InstrumentFuture::new(
            self.instrument.clone(),
            self.handle(),
            self.service.call(req),
        )
    }
}

// ===== impl WithPendingRequests =====

impl<D, I> WithPendingRequests<D, I> {
    pub fn new<Request>(discover: D, instrument: I) -> Self
    where
        D: Discover,
        D::Service: Service<Request>,
        I: Instrument<Handle, <D::Service as Service<Request>>::Response>,
    {
        Self {
            discover,
            instrument,
        }
    }
}

impl<D, I> Discover for WithPendingRequests<D, I>
where
    D: Discover,
    I: Clone,
{
    type Key = D::Key;
    type Service = PendingRequests<D::Service, I>;
    type Error = D::Error;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::Error> {
        use self::Change::*;

        let change = match try_ready!(self.discover.poll()) {
            Insert(k, svc) => Insert(k, PendingRequests::new(svc, self.instrument.clone())),
            Remove(k) => Remove(k),
        };

        Ok(Async::Ready(change))
    }
}

// ==== RefCount ====

impl RefCount {
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{future, Future, Poll};

    struct Svc;
    impl Service<()> for Svc {
        type Response = ();
        type Error = ();
        type Future = future::FutureResult<(), ()>;

        fn poll_ready(&mut self) -> Poll<(), ()> {
            Ok(().into())
        }

        fn call(&mut self, (): ()) -> Self::Future {
            future::ok(())
        }
    }

    #[test]
    fn default() {
        let mut svc = PendingRequests::new(Svc, NoInstrument);
        assert_eq!(svc.load(), Count(0));

        let rsp0 = svc.call(());
        assert_eq!(svc.load(), Count(1));

        let rsp1 = svc.call(());
        assert_eq!(svc.load(), Count(2));

        let () = rsp0.wait().unwrap();
        assert_eq!(svc.load(), Count(1));

        let () = rsp1.wait().unwrap();
        assert_eq!(svc.load(), Count(0));
    }

    #[test]
    fn instrumented() {
        #[derive(Clone)]
        struct IntoHandle;
        impl Instrument<Handle, ()> for IntoHandle {
            type Output = Handle;
            fn instrument(&self, i: Handle, (): ()) -> Handle {
                i
            }
        }

        let mut svc = PendingRequests::new(Svc, IntoHandle);
        assert_eq!(svc.load(), Count(0));

        let rsp = svc.call(());
        assert_eq!(svc.load(), Count(1));
        let i0 = rsp.wait().unwrap();
        assert_eq!(svc.load(), Count(1));

        let rsp = svc.call(());
        assert_eq!(svc.load(), Count(2));
        let i1 = rsp.wait().unwrap();
        assert_eq!(svc.load(), Count(2));

        drop(i1);
        assert_eq!(svc.load(), Count(1));

        drop(i0);
        assert_eq!(svc.load(), Count(0));
    }
}
