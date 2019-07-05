use futures::{Async, Future};
use tower_discover::ServiceList;
use tower_load as load;
use tower_service::Service;
use tower_test::mock;

use super::*;

macro_rules! assert_ready {
    ($svc:expr) => {{
        assert_ready!($svc, "must be ready");
    }};
    ($svc:expr, $msg:expr) => {{
        assert!($svc.poll_ready().expect("must not fail").is_ready(), $msg);
    }};
}

macro_rules! assert_not_ready {
    ($svc:expr) => {{
        assert_not_ready!($svc, "must not be ready");
    }};
    ($svc:expr, $msg:expr) => {{
        assert!(!$svc.poll_ready().expect("must not fail").is_ready(), $msg);
    }};
}

#[test]
fn empty() {
    let empty: Vec<load::Constant<mock::Mock<(), &'static str>, usize>> = vec![];
    let disco = ServiceList::new(empty);
    let mut svc = Balance::from_entropy(disco);
    with_task(|| {
        assert_not_ready!(svc);
    })
}

#[test]
fn single_endpoint() {
    let (mock, mut handle) = mock::pair();
    let mock = load::Constant::new(mock, 0);

    let disco = ServiceList::new(vec![mock].into_iter());
    let mut svc = Balance::from_entropy(disco);

    with_task(|| {
        handle.allow(0);
        assert_not_ready!(svc);
        assert_eq!(svc.len(), 1, "balancer must have discovered endpoint");

        handle.allow(1);
        assert_ready!(svc);

        let fut = svc.call(());

        let ((), rsp) = handle.next_request().unwrap();
        rsp.send_response(1);

        assert_eq!(fut.wait().expect("call must complete"), 1);
        handle.allow(1);
        assert_ready!(svc);

        handle.send_error("endpoint lost");
        assert_not_ready!(svc);
        assert!(svc.len() == 0, "balancer must drop failed endpoints");
    });
}

#[test]
fn two_endpoints_with_equal_load() {
    let (mock_a, mut handle_a) = mock::pair();
    let (mock_b, mut handle_b) = mock::pair();
    let mock_a = load::Constant::new(mock_a, 1);
    let mock_b = load::Constant::new(mock_b, 1);

    let disco = ServiceList::new(vec![mock_a, mock_b].into_iter());
    let mut svc = Balance::from_entropy(disco);

    with_task(|| {
        handle_a.allow(0);
        handle_b.allow(0);
        assert_not_ready!(svc);
        assert_eq!(svc.len(), 2, "balancer must have discovered both endpoints");

        handle_a.allow(1);
        handle_b.allow(0);
        assert_ready!(svc, "must be ready when one of two services is ready");
        {
            let fut = svc.call(());
            let ((), rsp) = handle_a.next_request().unwrap();
            rsp.send_response("a");
            assert_eq!(fut.wait().expect("call must complete"), "a");
        }

        handle_a.allow(0);
        handle_b.allow(1);
        assert_ready!(svc, "must be ready when both endpoints are ready");
        {
            let fut = svc.call(());
            let ((), rsp) = handle_b.next_request().unwrap();
            rsp.send_response("b");
            assert_eq!(fut.wait().expect("call must complete"), "b");
        }

        handle_a.allow(1);
        handle_b.allow(1);
        for _ in 0..2 {
            assert_ready!(svc, "must be ready when both endpoints are ready");
            let fut = svc.call(());
            for (ref mut h, c) in &mut [(&mut handle_a, "a"), (&mut handle_b, "b")] {
                if let Async::Ready(Some((_, tx))) = h.poll_request().unwrap() {
                    tracing::info!("using {}", c);
                    tx.send_response(c);
                    h.allow(0);
                }
            }
            fut.wait().expect("call must complete");
        }

        handle_a.send_error("endpoint lost");
        assert_not_ready!(svc, "must be not be ready");
        assert_eq!(svc.len(), 1, "balancer must drop failed endpoints",);
    });
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::lazy;
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
