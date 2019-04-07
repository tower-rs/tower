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

use super::{Balance, Choose};
use futures::{try_ready, Async, Future, Poll};
use tower_discover::{Change, Discover};
use tower_service::Service;
use tower_util::MakeService;

enum Load {
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
    load: Load,
    services: usize,
}

impl<MS, Target, Request> Discover for PoolDiscoverer<MS, Target, Request>
where
    MS: MakeService<Target, Request>,
    // NOTE: these bounds should go away once MakeService adopts Box<dyn Error>
    MS::MakeError: ::std::error::Error + Send + Sync + 'static,
    MS::Error: ::std::error::Error + Send + Sync + 'static,
    Target: Clone,
{
    type Key = usize;
    type Service = MS::Service;
    type Error = MS::MakeError;

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::Error> {
        if self.services == 0 && self.making.is_none() {
            self.making = Some(self.maker.make_service(self.target.clone()));
        }

        if let Load::High = self.load {
            if self.making.is_none() {
                try_ready!(self.maker.poll_ready());
                // TODO: it'd be great if we could avoid the clone here and use, say, &Target
                self.making = Some(self.maker.make_service(self.target.clone()));
            }
        }

        if let Some(mut fut) = self.making.take() {
            if let Async::Ready(s) = fut.poll()? {
                self.services += 1;
                self.load = Load::Normal;
                return Ok(Async::Ready(Change::Insert(self.services, s)));
            } else {
                self.making = Some(fut);
                return Ok(Async::NotReady);
            }
        }

        match self.load {
            Load::High => {
                unreachable!("found high load but no Service being made");
            }
            Load::Normal => Ok(Async::NotReady),
            Load::Low if self.services == 1 => Ok(Async::NotReady),
            Load::Low => {
                self.load = Load::Normal;
                let rm = self.services;
                self.services -= 1;
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
}

impl Default for Builder {
    fn default() -> Self {
        Builder {
            init: 0.1,
            low: 0.00001,
            high: 0.2,
            alpha: 0.03,
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

    /// See [`Pool::new`].
    pub fn build<C, MS, Target, Request>(
        &self,
        make_service: MS,
        target: Target,
        choose: C,
    ) -> Pool<C, MS, Target, Request>
    where
        MS: MakeService<Target, Request>,
        MS::MakeError: ::std::error::Error + Send + Sync + 'static,
        MS::Error: ::std::error::Error + Send + Sync + 'static,
        Target: Clone,
        C: Choose<usize, MS::Service>,
    {
        let d = PoolDiscoverer {
            maker: make_service,
            making: None,
            target,
            load: Load::Normal,
            services: 0,
        };

        Pool {
            balance: Balance::new(d, choose),
            options: *self,
            ewma: self.init,
        }
    }
}

/// A dynamically sized, load-balanced pool of `Service` instances.
pub struct Pool<C, MS, Target, Request>
where
    MS: MakeService<Target, Request>,
    MS::MakeError: ::std::error::Error + Send + Sync + 'static,
    MS::Error: ::std::error::Error + Send + Sync + 'static,
    Target: Clone,
{
    balance: Balance<PoolDiscoverer<MS, Target, Request>, C>,
    options: Builder,
    ewma: f64,
}

impl<C, MS, Target, Request> Pool<C, MS, Target, Request>
where
    MS: MakeService<Target, Request>,
    MS::MakeError: ::std::error::Error + Send + Sync + 'static,
    MS::Error: ::std::error::Error + Send + Sync + 'static,
    Target: Clone,
    C: Choose<usize, MS::Service>,
{
    /// Construct a new dynamically sized `Pool`.
    ///
    /// If many calls to `poll_ready` return `NotReady`, `new_service` is used to construct another
    /// `Service` that is then added to the load-balanced pool. If multiple services are available,
    /// `choose` is used to determine which one to use (just as in `Balance`). If many calls to
    /// `poll_ready` succeed, the most recently added `Service` is dropped from the pool.
    pub fn new(make_service: MS, target: Target, choose: C) -> Self {
        Builder::new().build(make_service, target, choose)
    }
}

impl<C, MS, Target, Request> Service<Request> for Pool<C, MS, Target, Request>
where
    MS: MakeService<Target, Request>,
    MS::MakeError: ::std::error::Error + Send + Sync + 'static,
    MS::Error: ::std::error::Error + Send + Sync + 'static,
    Target: Clone,
    C: Choose<usize, MS::Service>,
{
    type Response = <Balance<PoolDiscoverer<MS, Target, Request>, C> as Service<Request>>::Response;
    type Error = <Balance<PoolDiscoverer<MS, Target, Request>, C> as Service<Request>>::Error;
    type Future = <Balance<PoolDiscoverer<MS, Target, Request>, C> as Service<Request>>::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if let Async::Ready(()) = self.balance.poll_ready()? {
            // services was ready -- there are enough services
            // update ewma with a 0 sample
            self.ewma = (1.0 - self.options.alpha) * self.ewma;

            if self.ewma < self.options.low {
                self.balance.discover.load = Load::Low;

                if self.balance.discover.services > 1 {
                    // reset EWMA so we don't immediately try to remove another service
                    self.ewma = self.options.init;
                }
            } else {
                self.balance.discover.load = Load::Normal;
            }

            Ok(Async::Ready(()))
        } else if self.balance.discover.making.is_none() {
            // no services are ready -- we're overloaded
            // update ewma with a 1 sample
            self.ewma = self.options.alpha + (1.0 - self.options.alpha) * self.ewma;

            if self.ewma > self.options.high {
                self.balance.discover.load = Load::High;

            // don't reset the EWMA -- in theory, poll_ready should now start returning
            // `Ready`, so we won't try to launch another service immediately.
            } else {
                self.balance.discover.load = Load::Normal;
            }

            Ok(Async::NotReady)
        } else {
            // no services are ready, but we're already making another service!
            Ok(Async::NotReady)
        }
    }

    fn call(&mut self, req: Request) -> Self::Future {
        Service::call(&mut self.balance, req)
    }
}
