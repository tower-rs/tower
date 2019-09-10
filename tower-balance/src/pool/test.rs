use futures_util::pin_mut;
use tokio_test::{assert_pending, assert_ready, assert_ready_ok, task};
use tower_load as load;
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

use super::*;

#[test]
fn basic() {
    task::mock(|cx| {
        // start the pool
        let (mock, handle) =
            mock::pair::<(), load::Constant<mock::Mock<(), &'static str>, usize>>();
        pin_mut!(handle);

        let mut pool = Builder::new().build(mock, ());
        assert_pending!(pool.poll_ready(cx));

        // give the pool a backing service
        let (svc1_m, svc1) = mock::pair();
        pin_mut!(svc1);

        assert_request_eq!(handle, ()).send_response(load::Constant::new(svc1_m, 0));
        assert_ready_ok!(pool.poll_ready(cx));

        // send a request to the one backing service
        let fut = pool.call(());
        pin_mut!(fut);
        assert_pending!(fut.as_mut().poll(cx));
        assert_request_eq!(svc1, ()).send_response("foobar");
        assert_eq!(assert_ready_ok!(fut.poll(cx)), "foobar");
    });
}

#[test]
fn high_load() {
    task::mock(|cx| {
        // start the pool
        let (mock, handle) =
            mock::pair::<(), load::Constant<mock::Mock<(), &'static str>, usize>>();
        pin_mut!(handle);

        let mut pool = Builder::new()
            .urgency(1.0) // so _any_ NotReady will add a service
            .underutilized_below(0.0) // so no Ready will remove a service
            .max_services(Some(2))
            .build(mock, ());
        assert_pending!(pool.poll_ready(cx));

        // give the pool a backing service
        let (svc1_m, svc1) = mock::pair();
        pin_mut!(svc1);

        svc1.allow(1);
        assert_request_eq!(handle, ()).send_response(load::Constant::new(svc1_m, 0));
        assert_ready_ok!(pool.poll_ready(cx));

        // make the one backing service not ready
        let fut1 = pool.call(());
        pin_mut!(fut1);

        // if we poll_ready again, pool should notice that load is increasing
        // since urgency == 1.0, it should immediately enter high load
        assert_pending!(pool.poll_ready(cx));
        // it should ask the maker for another service, so we give it one
        let (svc2_m, svc2) = mock::pair();
        pin_mut!(svc2);

        svc2.allow(1);
        assert_request_eq!(handle, ()).send_response(load::Constant::new(svc2_m, 0));

        // the pool should now be ready again for one more request
        assert_ready_ok!(pool.poll_ready(cx));
        let fut2 = pool.call(());
        pin_mut!(fut2);
        assert_pending!(pool.poll_ready(cx));

        // the pool should _not_ try to add another service
        // sicen we have max_services(2)
        assert_pending!(handle.as_mut().poll_request(cx));

        // let see that each service got one request
        assert_request_eq!(svc1, ()).send_response("foo");
        assert_request_eq!(svc2, ()).send_response("bar");
        assert_eq!(assert_ready_ok!(fut1.poll(cx)), "foo");
        assert_eq!(assert_ready_ok!(fut2.poll(cx)), "bar");
    });
}

#[test]
fn low_load() {
    task::mock(|cx| {
        // start the pool
        let (mock, handle) =
            mock::pair::<(), load::Constant<mock::Mock<(), &'static str>, usize>>();
        pin_mut!(handle);

        let mut pool = Builder::new()
            .urgency(1.0) // so any event will change the service count
            .build(mock, ());
        assert_pending!(pool.poll_ready(cx));

        // give the pool a backing service
        let (svc1_m, svc1) = mock::pair();
        pin_mut!(svc1);

        svc1.allow(1);
        assert_request_eq!(handle, ()).send_response(load::Constant::new(svc1_m, 0));
        assert_ready_ok!(pool.poll_ready(cx));

        // cycling a request should now work
        let fut = pool.call(());
        pin_mut!(fut);

        assert_request_eq!(svc1, ()).send_response("foo");
        assert_eq!(assert_ready_ok!(fut.poll(cx)), "foo");
        // and pool should now not be ready (since svc1 isn't ready)
        // it should immediately try to add another service
        // which we give it
        assert_pending!(pool.poll_ready(cx));
        let (svc2_m, svc2) = mock::pair();
        pin_mut!(svc2);

        svc2.allow(1);
        assert_request_eq!(handle, ()).send_response(load::Constant::new(svc2_m, 0));
        // pool is now ready
        // which (because of urgency == 1.0) should immediately cause it to drop a service
        // it'll drop svc1, so it'll still be ready
        assert_ready_ok!(pool.poll_ready(cx));
        // and even with another ready, it won't drop svc2 since its now the only service
        assert_ready_ok!(pool.poll_ready(cx));

        // cycling a request should now work on svc2
        let fut = pool.call(());
        pin_mut!(fut);
        assert_request_eq!(svc2, ()).send_response("foo");
        assert_eq!(assert_ready_ok!(fut.poll(cx)), "foo");

        // and again (still svc2)
        svc2.allow(1);
        assert_ready_ok!(pool.poll_ready(cx));
        let fut = pool.call(());
        pin_mut!(fut);
        assert_request_eq!(svc2, ()).send_response("foo");
        assert_eq!(assert_ready_ok!(fut.poll(cx)), "foo");
    });
}

#[test]
fn failing_service() {
    task::mock(|cx| {
        // start the pool
        let (mock, handle) =
            mock::pair::<(), load::Constant<mock::Mock<(), &'static str>, usize>>();
        pin_mut!(handle);

        let mut pool = Builder::new()
            .urgency(1.0) // so _any_ NotReady will add a service
            .underutilized_below(0.0) // so no Ready will remove a service
            .build(mock, ());
        assert_pending!(pool.poll_ready(cx));

        // give the pool a backing service
        let (svc1_m, svc1) = mock::pair();
        pin_mut!(svc1);

        svc1.allow(1);
        assert_request_eq!(handle, ()).send_response(load::Constant::new(svc1_m, 0));
        assert_ready_ok!(pool.poll_ready(cx));

        // one request-response cycle
        let fut = pool.call(());
        pin_mut!(fut);

        assert_request_eq!(svc1, ()).send_response("foo");
        assert_eq!(assert_ready_ok!(fut.poll(cx)), "foo");

        // now make svc1 fail, so it has to be removed
        svc1.send_error("ouch");
        // polling now should recognize the failed service,
        // try to create a new one, and then realize the maker isn't ready
        assert_pending!(pool.poll_ready(cx));
        // then we release another service
        let (svc2_m, svc2) = mock::pair();
        pin_mut!(svc2);

        svc2.allow(1);
        assert_request_eq!(handle, ()).send_response(load::Constant::new(svc2_m, 0));

        // the pool should now be ready again
        assert_ready_ok!(pool.poll_ready(cx));
        // and a cycle should work (and go through svc2)
        let fut = pool.call(());
        pin_mut!(fut);

        assert_request_eq!(svc2, ()).send_response("bar");
        assert_eq!(assert_ready_ok!(fut.poll(cx)), "bar");
    });
}
