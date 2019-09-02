use crate::error;
use futures_util::{stream, try_future, StreamExt, TryFutureExt};
use indexmap::IndexMap;
use pin_project::pin_project;
use rand::{rngs::SmallRng, FromEntropy};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_sync::oneshot;
use tower_discover::{Change, Discover};
use tower_load::Load;
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

    ready_services: IndexMap<D::Key, D::Service>,

    unready_services: stream::FuturesUnordered<UnreadyService<D::Key, D::Service, Req>>,
    cancelations: IndexMap<D::Key, oneshot::Sender<()>>,

    /// Holds an index into `endpoints`, indicating the service that has been
    /// chosen to dispatch the next request.
    next_ready_index: Option<usize>,

    rng: SmallRng,
}

impl<D: Discover, Req> Unpin for Balance<D, Req> {}

/// A Future that becomes satisfied when an `S`-typed service is ready.
///
/// May fail due to cancelation, i.e. if the service is removed from discovery.
#[pin_project]
#[derive(Debug)]
struct UnreadyService<K, S, Req> {
    // This is a false positive
    key: Option<K>,
    #[pin]
    cancel: oneshot::Receiver<()>,
    #[pin]
    ready: Ready<S, Req>,
}

enum Error<E> {
    Inner(E),
    Canceled,
}

impl<D, Req> Balance<D, Req>
where
    D: Discover,
    D::Service: Service<Req>,
{
    /// Initializes a P2C load balancer from the provided randomization source.
    pub fn new(discover: D, rng: SmallRng) -> Self {
        Self {
            rng,
            discover,
            ready_services: IndexMap::default(),
            cancelations: IndexMap::default(),
            unready_services: stream::FuturesUnordered::new(),
            next_ready_index: None,
        }
    }

    /// Initializes a P2C load balancer from the OS's entropy source.
    pub fn from_entropy(discover: D) -> Self {
        Self::new(discover, SmallRng::from_entropy())
    }

    /// Returns the number of endpoints currently tracked by the balancer.
    pub fn len(&self) -> usize {
        self.ready_services.len() + self.unready_services.len()
    }

    // XXX `pool::Pool` requires direct access to this... Not ideal.
    pub(crate) fn discover_mut(&mut self) -> &mut D {
        &mut self.discover
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
    fn poll_discover(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), error::Discover>> {
        debug!("updating from discover");
        loop {
            match self.discover.poll(cx) {
                Poll::Ready(Ok(Change::Remove(key))) => {
                    trace!("remove");
                    self.evict(&key)
                }
                Poll::Ready(Ok(Change::Insert(key, svc))) => {
                    trace!("insert");
                    self.evict(&key);
                    self.push_unready(key, svc);
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(error::Discover(e.into()))),
                Poll::Pending => return Poll::Pending,
            }
        }
    }

    fn push_unready(&mut self, key: D::Key, svc: D::Service) {
        let (tx, rx) = oneshot::channel();
        self.cancelations.insert(key.clone(), tx);
        self.unready_services.push(UnreadyService {
            key: Some(key),
            ready: Ready::new(svc),
            cancel: rx,
        });
    }

    fn evict(&mut self, key: &D::Key) {
        // Update the ready index to account for reordering of ready.
        if let Some((idx, _, _)) = self.ready_services.swap_remove_full(key) {
            self.next_ready_index = self
                .next_ready_index
                .and_then(|i| Self::repair_index(i, idx, self.ready_services.len()));
            debug_assert!(!self.cancelations.contains_key(key));
        } else if let Some(cancel) = self.cancelations.remove(key) {
            let _ = cancel.send(());
        }
    }

    fn poll_unready(&mut self, cx: &mut Context<'_>) {
        loop {
            match self.unready_services.poll_next_unpin(cx) {
                Poll::Ready(None) | Poll::Pending => return,
                Poll::Ready(Some(Ok((key, svc)))) => {
                    trace!("endpoint ready");
                    let _cancel = self.cancelations.remove(&key);
                    debug_assert!(_cancel.is_some(), "missing cancelation");
                    self.ready_services.insert(key, svc);
                }
                Poll::Ready(Some(Err((key, Error::Canceled)))) => {
                    debug_assert!(!self.cancelations.contains_key(&key))
                }
                Poll::Ready(Some(Err((key, Error::Inner(e))))) => {
                    let error = e.into();
                    debug!({ %error }, "dropping failed endpoint");
                    let _cancel = self.cancelations.swap_remove(&key);
                    debug_assert!(_cancel.is_some());
                }
            }
        }
    }

    // Returns the updated index of `orig_idx` after the entry at `rm_idx` was
    // swap-removed from an IndexMap with `orig_sz` items.
    //
    // If `orig_idx` is the same as `rm_idx`, None is returned to indicate that
    // index cannot be repaired.
    fn repair_index(orig_idx: usize, rm_idx: usize, new_sz: usize) -> Option<usize> {
        debug_assert!(orig_idx <= new_sz && rm_idx <= new_sz);
        let repaired = match orig_idx {
            i if i == rm_idx => None,         // removed
            i if i == new_sz => Some(rm_idx), // swapped
            i => Some(i),                     // uneffected
        };
        trace!(
            { next.idx = orig_idx, removed.idx = rm_idx, length = new_sz, repaired.idx = ?repaired },
            "repairing index"
        );
        repaired
    }

    /// Performs P2C on inner services to find a suitable endpoint.
    fn p2c_next_ready_index(&mut self) -> Option<usize> {
        match self.ready_services.len() {
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

                trace!({ a.idx = aidx, a.load = ?aload, b.idx = bidx, b.load = ?bload, ready = ?ready }, "choosing by load");
                Some(ready)
            }
        }
    }

    /// Accesses a ready endpoint by index and returns its current load.
    fn ready_index_load(&self, index: usize) -> <D::Service as Load>::Metric {
        let (_, svc) = self.ready_services.get_index(index).expect("invalid index");
        svc.load()
    }

    fn poll_ready_index_or_evict(
        &mut self,
        cx: &mut Context<'_>,
        index: usize,
    ) -> Poll<Result<(), ()>> {
        let (_, svc) = self
            .ready_services
            .get_index_mut(index)
            .expect("invalid index");

        match svc.poll_ready(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Pending => {
                // became unready; so move it back there.
                let (key, svc) = self
                    .ready_services
                    .swap_remove_index(index)
                    .expect("invalid ready index");
                self.push_unready(key, svc);
                Poll::Pending
            }
            Poll::Ready(Err(e)) => {
                // failed, so drop it.
                let error = e.into();
                debug!({ %error }, "evicting failed endpoint");
                self.ready_services
                    .swap_remove_index(index)
                    .expect("invalid ready index");
                Poll::Ready(Err(()))
            }
        }
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
    type Future = try_future::MapErr<
        <D::Service as Service<Req>>::Future,
        fn(<D::Service as Service<Req>>::Error) -> error::Error,
    >;

    /// Prepares the balancer to process a request.
    ///
    /// When `Async::Ready` is returned, `ready_index` is set with a valid index
    /// into `ready` referring to a `Service` that is ready to disptach a request.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // First and foremost, process discovery updates. This removes or updates a
        // previously-selected `ready_index` if appropriate.
        let _ = self.poll_discover(cx)?;

        //Drive new or busy services to readiness.
        self.poll_unready(cx);

        trace!({ nready = self.ready_services.len(), nunready = self.unready_services.len() }, "poll_ready");

        loop {
            // If a node has already been selected, ensure that it is ready.
            // This ensures that the underlying service is ready immediately
            // before a request is dispatched to it. If, e.g., a failure
            // detector has changed the state of the service, it may be evicted
            // from the ready set so that P2C can be performed again.
            if let Some(index) = self.next_ready_index {
                trace!({ next.idx = index }, "preselected ready_index");
                debug_assert!(index < self.ready_services.len());

                if let Poll::Ready(Ok(())) = self.poll_ready_index_or_evict(cx, index) {
                    return Poll::Ready(Ok(()));
                }

                self.next_ready_index = None;
            }

            self.next_ready_index = self.p2c_next_ready_index();
            if self.next_ready_index.is_none() {
                debug_assert!(self.ready_services.is_empty());
                return Poll::Pending;
            }
        }
    }

    fn call(&mut self, request: Req) -> Self::Future {
        let index = self.next_ready_index.take().expect("not ready");
        let (key, mut svc) = self
            .ready_services
            .swap_remove_index(index)
            .expect("invalid ready index");
        // no need to repair since the ready_index has been cleared.

        let fut = svc.call(request);
        self.push_unready(key, svc);

        fut.map_err(Into::into)
    }
}

impl<K, S: Service<Req>, Req> Future for UnreadyService<K, S, Req> {
    type Output = Result<(K, S), (K, Error<S::Error>)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        if let Poll::Ready(Ok(())) = me.cancel.poll(cx) {
            let key = self.key.take().expect("polled after ready");
            return Poll::Ready(Err((key, Error::Canceled)));
        }

        match me.ready.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(svc)) => {
                let key = self.key.take().expect("polled after ready");
                Ok((key, svc)).into()
            }
            Poll::Ready(Err(e)) => {
                let key = self.key.take().expect("polled after ready");
                Err((key, Error::Inner(e))).into()
            }
        }
    }
}

#[derive(Debug)]
struct Ready<S, Req> {
    inner: Option<S>,
    _pd: PhantomData<Req>,
}

impl<S, Req> Ready<S, Req> {
    pub fn new(inner: S) -> Self {
        Self {
            inner: Some(inner),
            _pd: PhantomData,
        }
    }
}

impl<S, Req> Unpin for Ready<S, Req> {}

impl<S, Req> Future for Ready<S, Req>
where
    S: Service<Req>,
{
    type Output = Result<S, S::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(inner) = self.inner.as_mut() {
            match inner.poll_ready(cx) {
                Poll::Ready(Ok(())) => Poll::Ready(Ok(self.inner.take().unwrap())),
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            panic!("Polled after returne ready!")
        }
    }
}
