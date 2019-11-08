//! This module defines a load-balanced pool of services that adds new services when load is high.
//!
//! The pool uses `poll_ready` as a signal indicating whether additional services should be spawned
//! to handle the current level of load. Specifically, every time `poll_ready` on the inner service
//! returns `Ready`, [`Pool`] consider that a 0, and every time it returns `NotReady`, [`Pool`]
//! considers it a 1. [`Pool`] then maintains an [exponential moving
//! average](https://en.wikipedia.org/wiki/Moving_average#Exponential_moving_average) over those
//! samples, which gives an estimate of how often the underlying service has been ready when it was
//! needed "recently" (see [`Builder::urgency`]). If the service is loaded (see
//! [`Builder::loaded_above`]), a new service is created and added to the underlying [`Balance`].
//! If the service is underutilized (see [`Builder::underutilized_below`]) and there are two or
//! more services, then the latest added service is removed. In either case, the load estimate is
//! reset to its initial value (see [`Builder::initial`] to prevent services from being rapidly
//! added or removed.
#![deny(missing_docs)]

use super::p2c::Balance;
use crate::error;
use futures::{try_ready, Async, Future, Poll, Stream};
use slab::Slab;
use tower_discover::{Change, Discover};
use tower_load::Load;
use tower_service::Service;
use tower_util::MakeService;

#[cfg(test)]
mod test;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Level {
    /// Load is low -- remove a service instance.
    Low,
    /// Load is normal -- keep the service set as it is.
    Normal,
    /// Load is high -- add another service instance.
    High,
}

/// A wrapper around `MakeService` that discovers a new service when load is high, and removes a
/// service when load is low. See [`Pool`].
pub struct PoolDiscoverer<MS, Target, Request>
where
    MS: MakeService<Target, Request>,
{
    maker: MS,
    making: Option<MS::Future>,
    target: Target,
    load: Level,
    services: Slab<()>,
    died_tx: tokio_sync::mpsc::UnboundedSender<usize>,
    died_rx: tokio_sync::mpsc::UnboundedReceiver<usize>,
    limit: Option<usize>,
}

impl<MS, Target, Request> Discover for PoolDiscoverer<MS, Target, Request>
where
    MS: MakeService<Target, Request>,
    MS::MakeError: Into<error::Error>,
    MS::Error: Into<error::Error>,
    Target: Clone,
{
    type Key = usize;
    type Service = DropNotifyService<MS::Service>;
    type Error = MS::MakeError;

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::Error> {
        while let Async::Ready(Some(sid)) = self
            .died_rx
            .poll()
            .expect("cannot be closed as we hold tx too")
        {
            self.services.remove(sid);
            tracing::trace!(
                pool.services = self.services.len(),
                "removing dropped service"
            );
        }

        if self.services.len() == 0 && self.making.is_none() {
            let _ = try_ready!(self.maker.poll_ready());
            tracing::trace!("construct initial pool connection");
            self.making = Some(self.maker.make_service(self.target.clone()));
        }

        if let Level::High = self.load {
            if self.making.is_none() {
                if self
                    .limit
                    .map(|limit| self.services.len() >= limit)
                    .unwrap_or(false)
                {
                    return Ok(Async::NotReady);
                }

                tracing::trace!(
                    pool.services = self.services.len(),
                    "decided to add service to loaded pool"
                );
                try_ready!(self.maker.poll_ready());
                tracing::trace!("making new service");
                // TODO: it'd be great if we could avoid the clone here and use, say, &Target
                self.making = Some(self.maker.make_service(self.target.clone()));
            }
        }

        if let Some(mut fut) = self.making.take() {
            if let Async::Ready(svc) = fut.poll()? {
                let id = self.services.insert(());
                let svc = DropNotifyService {
                    svc,
                    id,
                    notify: self.died_tx.clone(),
                };
                tracing::trace!(
                    pool.services = self.services.len(),
                    "finished creating new service"
                );
                self.load = Level::Normal;
                return Ok(Async::Ready(Change::Insert(id, svc)));
            } else {
                self.making = Some(fut);
                return Ok(Async::NotReady);
            }
        }

        match self.load {
            Level::High => {
                unreachable!("found high load but no Service being made");
            }
            Level::Normal => Ok(Async::NotReady),
            Level::Low if self.services.len() == 1 => Ok(Async::NotReady),
            Level::Low => {
                self.load = Level::Normal;
                // NOTE: this is a little sad -- we'd prefer to kill short-living services
                let rm = self.services.iter().next().unwrap().0;
                // note that we _don't_ remove from self.services here
                // that'll happen automatically on drop
                tracing::trace!(
                    pool.services = self.services.len(),
                    "removing service for over-provisioned pool"
                );
                Ok(Async::Ready(Change::Remove(rm)))
            }
        }
    }
}

/// A [builder] that lets you configure how a [`Pool`] determines whether the underlying service is
/// loaded or not. See the [module-level documentation](index.html) and the builder's methods for
/// details.
///
///  [builder]: https://rust-lang-nursery.github.io/api-guidelines/type-safety.html#builders-enable-construction-of-complex-values-c-builder
#[derive(Copy, Clone, Debug)]
pub struct Builder {
    low: f64,
    high: f64,
    init: f64,
    alpha: f64,
    limit: Option<usize>,
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            init: 0.1,
            low: 0.00001,
            high: 0.2,
            alpha: 0.03,
            limit: None,
        }
    }
}

impl Builder {
    /// Create a new builder with default values for all load settings.
    ///
    /// If you just want to use the defaults, you can just use [`Pool::new`].
    pub fn new() -> Self {
        Self::default()
    }

    /// When the estimated load (see the [module-level docs](index.html)) drops below this
    /// threshold, and there are at least two services active, a service is removed.
    ///
    /// The default value is 0.01. That is, when one in every 100 `poll_ready` calls return
    /// `NotReady`, then the underlying service is considered underutilized.
    pub fn underutilized_below(&mut self, low: f64) -> &mut Self {
        self.low = low;
        self
    }

    /// When the estimated load (see the [module-level docs](index.html)) exceeds this
    /// threshold, and no service is currently in the process of being added, a new service is
    /// scheduled to be added to the underlying [`Balance`].
    ///
    /// The default value is 0.5. That is, when every other call to `poll_ready` returns
    /// `NotReady`, then the underlying service is considered highly loaded.
    pub fn loaded_above(&mut self, high: f64) -> &mut Self {
        self.high = high;
        self
    }

    /// The initial estimated load average.
    ///
    /// This is also the value that the estimated load will be reset to whenever a service is added
    /// or removed.
    ///
    /// The default value is 0.1.
    pub fn initial(&mut self, init: f64) -> &mut Self {
        self.init = init;
        self
    }

    /// How aggressively the estimated load average is updated.
    ///
    /// This is the α parameter of the formula for the [exponential moving
    /// average](https://en.wikipedia.org/wiki/Moving_average#Exponential_moving_average), and
    /// dictates how quickly new samples of the current load affect the estimated load. If the
    /// value is closer to 1, newer samples affect the load average a lot (when α is 1, the load
    /// average is immediately set to the current load). If the value is closer to 0, newer samples
    /// affect the load average very little at a time.
    ///
    /// The given value is clamped to `[0,1]`.
    ///
    /// The default value is 0.05, meaning, in very approximate terms, that each new load sample
    /// affects the estimated load by 5%.
    pub fn urgency(&mut self, alpha: f64) -> &mut Self {
        self.alpha = alpha.max(0.0).min(1.0);
        self
    }

    /// The maximum number of backing `Service` instances to maintain.
    ///
    /// When the limit is reached, the load estimate is clamped to the high load threshhold, and no
    /// new service is spawned.
    ///
    /// No maximum limit is imposed by default.
    pub fn max_services(&mut self, limit: Option<usize>) -> &mut Self {
        self.limit = limit;
        self
    }

    /// See [`Pool::new`].
    pub fn build<MS, Target, Request>(
        &self,
        make_service: MS,
        target: Target,
    ) -> Pool<MS, Target, Request>
    where
        MS: MakeService<Target, Request>,
        MS::Service: Load,
        <MS::Service as Load>::Metric: std::fmt::Debug,
        MS::MakeError: Into<error::Error>,
        MS::Error: Into<error::Error>,
        Target: Clone,
    {
        let (died_tx, died_rx) = tokio_sync::mpsc::unbounded_channel();
        let d = PoolDiscoverer {
            maker: make_service,
            making: None,
            target,
            load: Level::Normal,
            services: Slab::new(),
            died_tx,
            died_rx,
            limit: self.limit,
        };

        Pool {
            balance: Balance::from_entropy(d),
            options: *self,
            ewma: self.init,
        }
    }
}

/// A dynamically sized, load-balanced pool of `Service` instances.
pub struct Pool<MS, Target, Request>
where
    MS: MakeService<Target, Request>,
    MS::MakeError: Into<error::Error>,
    MS::Error: Into<error::Error>,
    Target: Clone,
{
    balance: Balance<PoolDiscoverer<MS, Target, Request>, Request>,
    options: Builder,
    ewma: f64,
}

impl<MS, Target, Request> Pool<MS, Target, Request>
where
    MS: MakeService<Target, Request>,
    MS::Service: Load,
    <MS::Service as Load>::Metric: std::fmt::Debug,
    MS::MakeError: Into<error::Error>,
    MS::Error: Into<error::Error>,
    Target: Clone,
{
    /// Construct a new dynamically sized `Pool`.
    ///
    /// If many calls to `poll_ready` return `NotReady`, `new_service` is used to
    /// construct another `Service` that is then added to the load-balanced pool.
    /// If many calls to `poll_ready` succeed, the most recently added `Service`
    /// is dropped from the pool.
    pub fn new(make_service: MS, target: Target) -> Self {
        Builder::new().build(make_service, target)
    }
}

impl<MS, Target, Req> Service<Req> for Pool<MS, Target, Req>
where
    MS: MakeService<Target, Req>,
    MS::Service: Load,
    <MS::Service as Load>::Metric: std::fmt::Debug,
    MS::MakeError: Into<error::Error>,
    MS::Error: Into<error::Error>,
    Target: Clone,
{
    type Response = <Balance<PoolDiscoverer<MS, Target, Req>, Req> as Service<Req>>::Response;
    type Error = <Balance<PoolDiscoverer<MS, Target, Req>, Req> as Service<Req>>::Error;
    type Future = <Balance<PoolDiscoverer<MS, Target, Req>, Req> as Service<Req>>::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if let Async::Ready(()) = self.balance.poll_ready()? {
            // services was ready -- there are enough services
            // update ewma with a 0 sample
            self.ewma = (1.0 - self.options.alpha) * self.ewma;

            let discover = self.balance.discover_mut();
            if self.ewma < self.options.low {
                if discover.load != Level::Low {
                    tracing::trace!(ewma = %self.ewma, "pool is over-provisioned");
                }
                discover.load = Level::Low;

                if discover.services.len() > 1 {
                    // reset EWMA so we don't immediately try to remove another service
                    self.ewma = self.options.init;
                }
            } else {
                if discover.load != Level::Normal {
                    tracing::trace!(ewma = %self.ewma, "pool is appropriately provisioned");
                }
                discover.load = Level::Normal;
            }

            return Ok(Async::Ready(()));
        }

        let discover = self.balance.discover_mut();
        if discover.making.is_none() {
            // no services are ready -- we're overloaded
            // update ewma with a 1 sample
            self.ewma = self.options.alpha + (1.0 - self.options.alpha) * self.ewma;

            if self.ewma > self.options.high {
                if discover.load != Level::High {
                    tracing::trace!(ewma = %self.ewma, "pool is under-provisioned");
                }
                discover.load = Level::High;

                // don't reset the EWMA -- in theory, poll_ready should now start returning
                // `Ready`, so we won't try to launch another service immediately.
                // we clamp it to high though in case the # of services is limited.
                self.ewma = self.options.high;

                // we need to call balance again for PoolDiscover to realize
                // it can make a new service
                return self.balance.poll_ready();
            } else {
                discover.load = Level::Normal;
            }
        }

        Ok(Async::NotReady)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        self.balance.call(req)
    }
}

#[doc(hidden)]
pub struct DropNotifyService<Svc> {
    svc: Svc,
    id: usize,
    notify: tokio_sync::mpsc::UnboundedSender<usize>,
}

impl<Svc> Drop for DropNotifyService<Svc> {
    fn drop(&mut self) {
        let _ = self.notify.try_send(self.id).is_ok();
    }
}

impl<Svc: Load> Load for DropNotifyService<Svc> {
    type Metric = Svc::Metric;
    fn load(&self) -> Self::Metric {
        self.svc.load()
    }
}

impl<Request, Svc: Service<Request>> Service<Request> for DropNotifyService<Svc> {
    type Response = Svc::Response;
    type Future = Svc::Future;
    type Error = Svc::Error;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.svc.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        self.svc.call(req)
    }
}
