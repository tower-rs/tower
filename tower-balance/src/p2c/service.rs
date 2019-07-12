use crate::error;
use futures::{future, try_ready, Async, Future, Poll};
use log::{debug, trace};
use rand::{rngs::SmallRng, FromEntropy};
use tower_discover::{Change, Discover};
use tower_load::Load;
use tower_ready_cache::ReadyCache;
use tower_service::Service;

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

    ready_services: ReadyCache<D::Key, D::Service, Req>,

    /// Holds an index into `endpoints`, indicating the service that has been
    /// chosen to dispatch the next request.
    next_ready_index: Option<usize>,

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
            ready_services: ReadyCache::default(),
            next_ready_index: None,
        }
    }

    /// Initializes a P2C load balancer from the OS's entropy source.
    pub fn from_entropy(discover: D) -> Self {
        Self::new(discover, SmallRng::from_entropy())
    }

    /// Returns the number of endpoints currently tracked by the balancer.
    pub fn len(&self) -> usize {
        self.ready_services.ready_len() + self.ready_services.unready_len()
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
    pub(crate) fn discover_mut(&mut self) -> &mut D {
        &mut self.discover
    }

    /// Polls `discover` for updates, adding new items to `not_ready`.
    ///
    /// Removals may alter the order of either `ready` or `not_ready`.
    fn poll_discover(&mut self) -> Poll<(), error::Discover> {
        debug!("updating from discover");
        loop {
            match try_ready!(self.discover.poll().map_err(|e| error::Discover(e.into()))) {
                Change::Remove(key) => {
                    trace!("remove");
                    if let Some(idx) = self.ready_services.evict(&key) {
                        self.repair_next_ready_index(idx);
                    }
                }
                Change::Insert(key, svc) => {
                    trace!("insert");
                    if let Some(idx) = self.ready_services.evict(&key) {
                        self.repair_next_ready_index(idx);
                    }
                    self.ready_services.push_unready(key, svc);
                }
            }
        }
    }

    fn poll_unready(&mut self) {
        self.ready_services.poll_unready();
        trace!(
            "ready={}; unready={}",
            self.ready_services.ready_len(),
            self.ready_services.unready_len(),
        );
    }

    /// Performs P2C on inner services to find a suitable endpoint.
    fn p2c_next_ready_index(&mut self) -> Option<usize> {
        match self.ready_services.ready_len() {
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
        let (_, svc) = self
            .ready_services
            .get_ready_index(index)
            .expect("invalid index");
        svc.load()
    }

    // Updates `next_ready_index_idx` after the entry at `rm_idx` was evicted.
    fn repair_next_ready_index(&mut self, rm_idx: usize) {
        self.next_ready_index = self.next_ready_index.take().and_then(|idx| {
            let new_sz = self.ready_services.ready_len();
            debug_assert!(idx <= new_sz && rm_idx <= new_sz);
            let repaired = match idx {
                i if i == rm_idx => None,         // removed
                i if i == new_sz => Some(rm_idx), // swapped
                i => Some(i),                     // uneffected
            };
            trace!(
                "repair_index: orig={}; rm={}; sz={}; => {:?}",
                idx,
                rm_idx,
                new_sz,
                repaired,
            );
            repaired
        });
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

        loop {
            // If a node has already been selected, ensure that it is ready.
            // This ensures that the underlying service is ready immediately
            // before a request is dispatched to it. If, e.g., a failure
            // detector has changed the state of the service, it may be evicted
            // from the ready set so that P2C can be performed again.
            if let Some(index) = self.next_ready_index {
                trace!("preselected ready_index={}", index);
                debug_assert!(index < self.ready_services.ready_len());

                if let Ok(Async::Ready(())) = self.ready_services.poll_ready_index(index) {
                    return Ok(Async::Ready(()));
                }

                self.next_ready_index = None;
            }

            self.next_ready_index = self.p2c_next_ready_index();
            if self.next_ready_index.is_none() {
                debug_assert_eq!(self.ready_services.ready_len(), 0);
                return Ok(Async::NotReady);
            }
        }
    }

    fn call(&mut self, request: Req) -> Self::Future {
        let index = self.next_ready_index.take().expect("not ready");
        self.ready_services
            .process_ready_index(index, move |svc| svc.call(request))
            .map_err(Into::into)
    }
}
