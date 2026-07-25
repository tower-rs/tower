use crate::discover::ServiceList;
use crate::load;
use futures_util::pin_mut;
use std::task::Poll;
use tokio_test::{assert_pending, assert_ready, assert_ready_ok, task};
use tower_test::{assert_request_eq, mock};

use super::*;

#[tokio::test]
async fn empty() {
    let empty: Vec<load::Constant<mock::Mock<(), &'static str>, usize>> = vec![];
    let disco = ServiceList::new(empty);
    let mut svc = mock::Spawn::new(Balance::new(disco));
    assert_pending!(svc.poll_ready());
}

#[tokio::test]
async fn single_endpoint() {
    let (mut svc, mut handle) = mock::spawn_with(|s| {
        let mock = load::Constant::new(s, 0);
        let disco = ServiceList::new(vec![mock].into_iter());
        Balance::new(disco)
    });

    handle.allow(0);
    assert_pending!(svc.poll_ready());
    assert_eq!(
        svc.get_ref().len(),
        1,
        "balancer must have discovered endpoint"
    );

    handle.allow(1);
    assert_ready_ok!(svc.poll_ready());

    let mut fut = task::spawn(svc.call(()));

    assert_request_eq!(handle, ()).send_response(1);

    assert_eq!(assert_ready_ok!(fut.poll()), 1);
    handle.allow(1);
    assert_ready_ok!(svc.poll_ready());

    handle.send_error("endpoint lost");
    assert_pending!(svc.poll_ready());
    assert!(
        svc.get_ref().is_empty(),
        "balancer must drop failed endpoints"
    );
}

#[tokio::test]
async fn two_endpoints_with_equal_load() {
    let (mock_a, handle_a) = mock::pair();
    let (mock_b, handle_b) = mock::pair();
    let mock_a = load::Constant::new(mock_a, 1);
    let mock_b = load::Constant::new(mock_b, 1);

    pin_mut!(handle_a);
    pin_mut!(handle_b);

    let disco = ServiceList::new(vec![mock_a, mock_b].into_iter());
    let mut svc = mock::Spawn::new(Balance::new(disco));

    handle_a.allow(0);
    handle_b.allow(0);
    assert_pending!(svc.poll_ready());
    assert_eq!(
        svc.get_ref().len(),
        2,
        "balancer must have discovered both endpoints"
    );

    handle_a.allow(1);
    handle_b.allow(0);
    assert_ready_ok!(
        svc.poll_ready(),
        "must be ready when one of two services is ready"
    );
    {
        let mut fut = task::spawn(svc.call(()));
        assert_request_eq!(handle_a, ()).send_response("a");
        assert_eq!(assert_ready_ok!(fut.poll()), "a");
    }

    handle_a.allow(0);
    handle_b.allow(1);
    assert_ready_ok!(
        svc.poll_ready(),
        "must be ready when both endpoints are ready"
    );
    {
        let mut fut = task::spawn(svc.call(()));
        assert_request_eq!(handle_b, ()).send_response("b");
        assert_eq!(assert_ready_ok!(fut.poll()), "b");
    }

    handle_a.allow(1);
    handle_b.allow(1);
    for _ in 0..2 {
        assert_ready_ok!(
            svc.poll_ready(),
            "must be ready when both endpoints are ready"
        );
        let mut fut = task::spawn(svc.call(()));

        for (ref mut h, c) in &mut [(&mut handle_a, "a"), (&mut handle_b, "b")] {
            if let Poll::Ready(Some((_, tx))) = h.as_mut().poll_request() {
                tracing::info!("using {}", c);
                tx.send_response(c);
                h.allow(0);
            }
        }
        assert_ready_ok!(fut.poll());
    }

    handle_a.send_error("endpoint lost");
    assert_pending!(svc.poll_ready());
    assert_eq!(
        svc.get_ref().len(),
        1,
        "balancer must drop failed endpoints",
    );
}

// Regression test for #856: a discovery removal must invalidate the cached P2C
// `ready_index`. Otherwise `ReadyCache::evict` (which uses `swap_remove`) can
// move a different endpoint into the cached slot, and the next request is
// dispatched to that swapped-in endpoint instead of a freshly-selected one.
#[tokio::test]
async fn ready_index_reset_after_removal() {
    use crate::discover::Change;
    use crate::util::rng::Rng;
    use crate::Service;
    use futures_core::Stream;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use std::future::{ready, Ready};
    use std::pin::Pin;
    use std::rc::Rc;
    use std::task::Context;

    // An always-ready endpoint that reports its own id as the response, so we
    // can observe which endpoint a request was routed to.
    #[derive(Clone)]
    struct IdSvc(usize);
    impl Service<()> for IdSvc {
        type Response = usize;
        type Error = Infallible;
        type Future = Ready<Result<usize, Infallible>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: ()) -> Self::Future {
            ready(Ok(self.0))
        }
    }

    type Endpoint = load::Constant<IdSvc, usize>;

    // A `Discover` we feed one change at a time, so endpoints are promoted to
    // the ready set in a deterministic order.
    struct Disco(Rc<RefCell<VecDeque<Change<usize, Endpoint>>>>);
    impl Stream for Disco {
        type Item = Result<Change<usize, Endpoint>, Infallible>;

        fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            match self.0.borrow_mut().pop_front() {
                Some(change) => Poll::Ready(Some(Ok(change))),
                None => Poll::Pending,
            }
        }
    }

    // A zero RNG makes P2C deterministically sample ready indices 0 and len-1.
    struct ZeroRng;
    impl Rng for ZeroRng {
        fn next_u64(&mut self) -> u64 {
            0
        }
    }

    let endpoint = |id: usize, load_val: usize| load::Constant::new(IdSvc(id), load_val);

    let changes: Rc<RefCell<VecDeque<Change<usize, Endpoint>>>> =
        Rc::new(RefCell::new(VecDeque::new()));
    let mut svc = mock::Spawn::new(Balance::from_rng(Disco(changes.clone()), ZeroRng));

    // Promote endpoints one per poll so the ready set is deterministically
    // ordered [a@0, b@1, c@2]. P2C selects `a` first (the only ready service)
    // and keeps it cached while it stays ready.
    changes
        .borrow_mut()
        .push_back(Change::Insert(0, endpoint(0, 0)));
    assert_ready_ok!(svc.poll_ready());
    changes
        .borrow_mut()
        .push_back(Change::Insert(1, endpoint(1, 1)));
    assert_ready_ok!(svc.poll_ready());
    changes
        .borrow_mut()
        .push_back(Change::Insert(2, endpoint(2, 10)));
    assert_ready_ok!(svc.poll_ready());

    // Remove the cached endpoint `a`. `evict` swap-removes index 0 and moves the
    // last ready service (`c`) into that slot. Before the fix, the stale
    // `ready_index` (0) is revalidated against `c` and the request is routed to
    // `c`; after the fix, `ready_index` is cleared and P2C runs afresh.
    changes.borrow_mut().push_back(Change::Remove(0));
    assert_ready_ok!(svc.poll_ready());

    // A fresh P2C over [c@0 (load 10), b@1 (load 1)] picks `b`. The stale-index
    // bug would route to `c` instead.
    let mut fut = task::spawn(svc.call(()));
    assert_eq!(
        assert_ready_ok!(fut.poll()),
        1,
        "request must be routed by a fresh P2C selection (b), not the endpoint \
         swapped into the stale ready_index (c)"
    );
}
