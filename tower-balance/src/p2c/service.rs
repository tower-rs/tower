use crate::error;
use futures::{future, try_ready, Async, Future, Poll};
use rand::{rngs::SmallRng, FromEntropy};
use tower_discover::{Change, Discover};
use tower_load::Load;
use tower_ready_cache::ReadyCache;
use tower_service::Service;
use tracing::{debug, trace};

/// Distributes requests across inner services using the [Power of Two Choices][p2c].
///
/// As described in the [Finagle Guide][finagle]:
///
/// > The algorithm randomly picks two services from the set of ready endpoints and
/// > selects the least loaded of the two. By repeatedly using this strategy, we can
/// > expect a manageable upper bound on the maximum load of any server.
/// >
/// > The maximum load variance between any two servers is bound by `ln(ln(n))` where
/// > `n` is the number of servers in the cluster.
///
/// [finagle]: https://twitter.github.io/finagle/guide/Clients.html#power-of-two-choices-p2c-least-loaded
/// [p2c]: http://www.eecs.harvard.edu/~michaelm/postscripts/handbook2001.pdf
#[derive(Debug)]
pub struct Balance<D: Discover, Req> {
    discover: D,

    services: ReadyCache<D::Key, D::Service, Req>,

    rng: SmallRng,
}

impl<D, Req> Balance<D, Req>
where
    D: Discover,
    D::Service: Service<Req>,
    <D::Service as Service<Req>>::Error: Into<error::Error>,
{
    /// Initializes a P2C load balancer from the provided randomization source.
    pub fn new(discover: D, rng: SmallRng) -> Self {
        Self {
            rng,
            discover,
            services: ReadyCache::default(),
        }
    }

    /// Initializes a P2C load balancer from the OS's entropy source.
    pub fn from_entropy(discover: D) -> Self {
        Self::new(discover, SmallRng::from_entropy())
    }

    /// Returns the number of endpoints currently tracked by the balancer.
    pub fn len(&self) -> usize {
        self.services.len()
    }
}

impl<D, Req> Balance<D, Req>
where
    D: Discover,
    D::Key: Clone,
    D::Error: Into<error::Error>,
    D::Service: Service<Req> + Load,
    <D::Service as Load>::Metric: std::fmt::Debug,
    <D::Service as Service<Req>>::Error: Into<error::Error>,
{
    /// Polls `discover` for updates, adding new items to `not_ready`.
    ///
    /// Removals may alter the order of either `ready` or `not_ready`.
    fn poll_discover(&mut self) -> Poll<(), error::Discover> {
        debug!("updating from discover");
        loop {
            match try_ready!(self.discover.poll().map_err(|e| error::Discover(e.into()))) {
                Change::Remove(key) => {
                    trace!("remove");
                    self.services.evict(&key);
                }
                Change::Insert(key, svc) => {
                    trace!("insert");
                    // If this service already existed in the set, it will be
                    // replaced as the new one becomes ready.
                    self.services.push_service(key, svc);
                }
            }
        }
    }

    fn poll_unready(&mut self) {
        // Note: this cannot perturb the order of ready services.
        loop {
            if let Err(e) = self.services.poll_pending() {
                debug!("dropping endpoint: {:?}", e);
            } else {
                break;
            }
        }
        trace!(
            "ready={}; pending={}",
            self.services.ready_len(),
            self.services.pending_len(),
        );
    }

    /// Performs P2C on inner services to find a suitable endpoint.
    fn p2c_next_ready_index(&mut self) -> Option<usize> {
        match self.services.ready_len() {
            0 => None,
            1 => Some(0),
            len => {
                // Get two distinct random indexes (in a random order) and
                // compare the loads of the service at each index.
                let idxs = rand::seq::index::sample(&mut self.rng, len, 2);

                let aidx = idxs.index(0);
                let bidx = idxs.index(1);
                debug_assert_ne!(aidx, bidx, "random indices must be distinct");

                let aload = self.ready_index_load(aidx);
                let bload = self.ready_index_load(bidx);
                let ready = if aload <= bload { aidx } else { bidx };

                trace!(
                    "load[{}]={:?}; load[{}]={:?} -> ready={}",
                    aidx,
                    aload,
                    bidx,
                    bload,
                    ready
                );
                Some(ready)
            }
        }
    }

    /// Accesses a ready endpoint by index and returns its current load.
    fn ready_index_load(&self, index: usize) -> <D::Service as Load>::Metric {
        let (_, svc) = self.services.get_ready_index(index).expect("invalid index");
        svc.load()
    }
}

impl<D, Req> Service<Req> for Balance<D, Req>
where
    D: Discover,
    D::Key: Clone,
    D::Error: Into<error::Error>,
    D::Service: Service<Req> + Load,
    <D::Service as Load>::Metric: std::fmt::Debug,
    <D::Service as Service<Req>>::Error: Into<error::Error>,
{
    type Response = <D::Service as Service<Req>>::Response;
    type Error = error::Error;
    type Future = future::MapErr<
        <D::Service as Service<Req>>::Future,
        fn(<D::Service as Service<Req>>::Error) -> error::Error,
    >;

    /// Prepares the balancer to process a request.
    ///
    /// When `Async::Ready` is returned, `ready_index` is set with a valid index
    /// into `ready` referring to a `Service` that is ready to disptach a request.
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // First and foremost, process discovery updates. This removes or updates a
        // previously-selected `ready_index` if appropriate.
        self.poll_discover()?;

        // Drive new or busy services to readiness.
        self.poll_unready();

        let mut index = self.services.take_next_ready_index();
        loop {
            // If a service has already been selected, ensure that it is ready.
            // This ensures that the underlying service is ready immediately
            // before a request is dispatched to it. If, e.g., a failure
            // detector has changed the state of the service, it may be evicted
            // from the ready set so that another service can be selected.
            if let Some(index) = index {
                if let Ok(Async::Ready(())) = self.services.poll_next_ready_index(index) {
                    return Ok(Async::Ready(()));
                }
            }

            index = self.p2c_next_ready_index();
            if index.is_none() {
                debug_assert_eq!(self.services.ready_len(), 0);
                return Ok(Async::NotReady);
            }
        }
    }

    fn call(&mut self, request: Req) -> Self::Future {
        self.services.call_next_ready(request).map_err(Into::into)
    }
}
