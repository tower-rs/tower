use futures_util::pin_mut;
use std::{future::Future, task::Poll};
use tokio_test::{assert_pending, assert_ready, assert_ready_ok, task};
use tower_discover::ServiceList;
use tower_load as load;
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

use super::*;

#[test]
fn empty() {
    task::mock(|cx| {
        let empty: Vec<load::Constant<mock::Mock<(), &'static str>, usize>> = vec![];
        let disco = ServiceList::new(empty);
        let mut svc = Balance::from_entropy(disco);
        assert_pending!(svc.poll_ready(cx));
    });
}

#[test]
fn single_endpoint() {
    task::mock(|cx| {
        let (mock, handle) = mock::pair();
        pin_mut!(handle);

        let mock = load::Constant::new(mock, 0);

        let disco = ServiceList::new(vec![mock].into_iter());
        let mut svc = Balance::from_entropy(disco);

        handle.allow(0);
        assert_pending!(svc.poll_ready(cx));
        assert_eq!(svc.len(), 1, "balancer must have discovered endpoint");

        handle.allow(1);
        assert_ready_ok!(svc.poll_ready(cx));

        let fut = svc.call(());
        pin_mut!(fut);

        assert_request_eq!(handle, ()).send_response(1);

        assert_eq!(assert_ready_ok!(fut.poll(cx)), 1);
        handle.allow(1);
        assert_ready_ok!(svc.poll_ready(cx));

        handle.send_error("endpoint lost");
        assert_pending!(svc.poll_ready(cx));
        assert!(svc.len() == 0, "balancer must drop failed endpoints");
    });
}

#[test]
fn two_endpoints_with_equal_load() {
    task::mock(|cx| {
        let (mock_a, handle_a) = mock::pair();
        let (mock_b, handle_b) = mock::pair();
        let mock_a = load::Constant::new(mock_a, 1);
        let mock_b = load::Constant::new(mock_b, 1);

        pin_mut!(handle_a);
        pin_mut!(handle_b);

        let disco = ServiceList::new(vec![mock_a, mock_b].into_iter());
        let mut svc = Balance::from_entropy(disco);

        handle_a.allow(0);
        handle_b.allow(0);
        assert_pending!(svc.poll_ready(cx));
        assert_eq!(svc.len(), 2, "balancer must have discovered both endpoints");

        handle_a.allow(1);
        handle_b.allow(0);
        assert_ready_ok!(
            svc.poll_ready(cx),
            "must be ready when one of two services is ready"
        );
        {
            let fut = svc.call(());
            pin_mut!(fut);
            assert_request_eq!(handle_a, ()).send_response("a");
            assert_eq!(assert_ready_ok!(fut.poll(cx)), "a");
        }

        handle_a.allow(0);
        handle_b.allow(1);
        assert_ready_ok!(
            svc.poll_ready(cx),
            "must be ready when both endpoints are ready"
        );
        {
            let fut = svc.call(());
            pin_mut!(fut);
            assert_request_eq!(handle_b, ()).send_response("b");
            assert_eq!(assert_ready_ok!(fut.poll(cx)), "b");
        }

        handle_a.allow(1);
        handle_b.allow(1);
        for _ in 0..2 {
            assert_ready_ok!(
                svc.poll_ready(cx),
                "must be ready when both endpoints are ready"
            );
            let fut = svc.call(());
            pin_mut!(fut);
            for (ref mut h, c) in &mut [(&mut handle_a, "a"), (&mut handle_b, "b")] {
                if let Poll::Ready(Some((_, tx))) = h.as_mut().poll_request(cx) {
                    tracing::info!("using {}", c);
                    tx.send_response(c);
                    h.allow(0);
                }
            }
            assert_ready_ok!(fut.as_mut().poll(cx));
        }

        handle_a.send_error("endpoint lost");
        assert_pending!(svc.poll_ready(cx));
        assert_eq!(svc.len(), 1, "balancer must drop failed endpoints",);
    });
}
