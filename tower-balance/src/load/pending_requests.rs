use futures::{Async, Poll};
use std::marker::PhantomData;
use std::sync::Arc;
use tower_discover::{Change, Discover};
use tower_service::Service;

use Load;
use super::{Measure, MeasureFuture, NoMeasure};

/// Expresses load based on the number of currently-pending requests.
#[derive(Debug)]
pub struct PendingRequests<S, M = NoMeasure>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    service: S,
    ref_count: RefCount,
    _p: PhantomData<M>,
}

/// Shared between instances of `PendingRequests` and `Instrument` to track active
/// references.
#[derive(Clone, Debug, Default)]
struct RefCount(Arc<()>);

/// Wraps `inner`'s services with `PendingRequests`.
#[derive(Debug)]
pub struct WithPendingRequests<D, M = NoMeasure>
where
    D: Discover,
    M: Measure<Instrument, D::Response>,
{
    discover: D,
    _p: PhantomData<M>,
}

/// Represents the number of currently-pending requests to a given service.
#[derive(Clone, Copy, Debug, Default, PartialOrd, PartialEq, Ord, Eq)]
pub struct Count(usize);

#[derive(Debug)]
pub struct Instrument(RefCount);

// ===== impl PendingRequests =====

impl<S: Service> PendingRequests<S, NoMeasure> {
    pub fn new(service: S) -> Self {
        Self {
            service,
            ref_count: RefCount::default(),
            _p: PhantomData,
        }
    }

    /// Configures the load metric to be determined with the provided measurement strategy.
    pub fn measured<M>(self) -> PendingRequests<S, M>
    where
        M: Measure<Instrument, S::Response>,
    {
        PendingRequests {
            service: self.service,
            ref_count: self.ref_count,
            _p: PhantomData,
        }
    }
}

impl<S, M> PendingRequests<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    fn instrument(&self) -> Instrument {
        Instrument(self.ref_count.clone())
    }
}

impl<S, M> Load for PendingRequests<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    type Metric = Count;

    fn load(&self) -> Count {
        // Count the number of references that aren't `self`.
        Count(self.ref_count.ref_count() - 1)
    }
}

impl<S, M> Service for PendingRequests<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    type Request = S::Request;
    type Response = M::Measured;
    type Error = S::Error;
    type Future = MeasureFuture<S::Future, M, Instrument>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        MeasureFuture::new(self.instrument(), self.service.call(req))
    }
}

// ===== impl WithPendingRequests =====

impl<D> WithPendingRequests<D, NoMeasure>
where
    D: Discover,
{
    pub fn new(discover: D) -> Self {
        Self {
            discover,
            _p: PhantomData,
        }
    }

    pub fn measured<M>(self) -> WithPendingRequests<D, M>
    where
        M: Measure<Instrument, D::Response>,
    {
        WithPendingRequests {
            discover: self.discover,
            _p: PhantomData,
        }
    }
}

impl<D, M> Discover for WithPendingRequests<D, M>
where
    D: Discover,
    M: Measure<Instrument, D::Response>,
{
    type Key = D::Key;
    type Request = D::Request;
    type Response = M::Measured;
    type Error = D::Error;
    type Service = PendingRequests<D::Service, M>;
    type DiscoverError = D::DiscoverError;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::DiscoverError> {
        use self::Change::*;

        let change = match try_ready!(self.discover.poll()) {
            Insert(k, svc) => Insert(k, PendingRequests::new(svc).measured()),
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
    use futures::{Future, Poll, future};
    use super::*;

    struct Svc;
    impl Service for Svc {
        type Request = ();
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
        let mut svc = PendingRequests::new(Svc);
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
    fn measured() {
        struct IntoInstrument;
        impl Measure<Instrument, ()> for IntoInstrument {
            type Measured = Instrument;
            fn measure(i: Instrument, (): ()) -> Instrument {
                i
            }
        }

        let mut svc = PendingRequests::new(Svc).measured::<IntoInstrument>();
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
