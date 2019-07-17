use futures::{Async, Future};
use tower_load as load;
use tower_service::Service;
use tower_test::mock;

use super::*;

macro_rules! assert_fut_ready {
    ($fut:expr, $val:expr) => {{
        assert_fut_ready!($fut, $val, "must be ready");
    }};
    ($fut:expr, $val:expr, $msg:expr) => {{
        assert_eq!(
            $fut.poll().expect("must not fail"),
            Async::Ready($val),
            $msg
        );
    }};
}

macro_rules! assert_ready {
    ($svc:expr) => {{
        assert_ready!($svc, "must be ready");
    }};
    ($svc:expr, $msg:expr) => {{
        assert!($svc.poll_ready().expect("must not fail").is_ready(), $msg);
    }};
}

macro_rules! assert_fut_not_ready {
    ($fut:expr) => {{
        assert_fut_not_ready!($fut, "must not be ready");
    }};
    ($fut:expr, $msg:expr) => {{
        assert!(!$fut.poll().expect("must not fail").is_ready(), $msg);
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
fn basic() {
    // start the pool
    let (mock, mut handle) =
        mock::pair::<(), load::Constant<mock::Mock<(), &'static str>, usize>>();
    let mut pool = Builder::new().build(mock, ());
    with_task(|| {
        assert_not_ready!(pool);
    });

    // give the pool a backing service
    let (svc1_m, mut svc1) = mock::pair();
    handle
        .next_request()
        .unwrap()
        .1
        .send_response(load::Constant::new(svc1_m, 0));
    with_task(|| {
        assert_ready!(pool);
    });

    // send a request to the one backing service
    let mut fut = pool.call(());
    with_task(|| {
        assert_fut_not_ready!(fut);
    });
    svc1.next_request().unwrap().1.send_response("foobar");
    with_task(|| {
        assert_fut_ready!(fut, "foobar");
    });
}

#[test]
fn high_load() {
    // start the pool
    let (mock, mut handle) =
        mock::pair::<(), load::Constant<mock::Mock<(), &'static str>, usize>>();
    let mut pool = Builder::new()
        .urgency(1.0) // so _any_ NotReady will add a service
        .underutilized_below(0.0) // so no Ready will remove a service
        .max_services(Some(2))
        .build(mock, ());
    with_task(|| {
        assert_not_ready!(pool);
    });

    // give the pool a backing service
    let (svc1_m, mut svc1) = mock::pair();
    svc1.allow(1);
    handle
        .next_request()
        .unwrap()
        .1
        .send_response(load::Constant::new(svc1_m, 0));
    with_task(|| {
        assert_ready!(pool);
    });

    // make the one backing service not ready
    let mut fut1 = pool.call(());

    // if we poll_ready again, pool should notice that load is increasing
    // since urgency == 1.0, it should immediately enter high load
    with_task(|| {
        assert_not_ready!(pool);
    });
    // it should ask the maker for another service, so we give it one
    let (svc2_m, mut svc2) = mock::pair();
    svc2.allow(1);
    handle
        .next_request()
        .unwrap()
        .1
        .send_response(load::Constant::new(svc2_m, 0));

    // the pool should now be ready again for one more request
    with_task(|| {
        assert_ready!(pool);
    });
    let mut fut2 = pool.call(());
    with_task(|| {
        assert_not_ready!(pool);
    });

    // the pool should _not_ try to add another service
    // sicen we have max_services(2)
    with_task(|| {
        assert!(!handle.poll_request().unwrap().is_ready());
    });

    // let see that each service got one request
    svc1.next_request().unwrap().1.send_response("foo");
    svc2.next_request().unwrap().1.send_response("bar");
    with_task(|| {
        assert_fut_ready!(fut1, "foo");
    });
    with_task(|| {
        assert_fut_ready!(fut2, "bar");
    });
}

#[test]
fn low_load() {
    // start the pool
    let (mock, mut handle) =
        mock::pair::<(), load::Constant<mock::Mock<(), &'static str>, usize>>();
    let mut pool = Builder::new()
        .urgency(1.0) // so any event will change the service count
        .build(mock, ());
    with_task(|| {
        assert_not_ready!(pool);
    });

    // give the pool a backing service
    let (svc1_m, mut svc1) = mock::pair();
    svc1.allow(1);
    handle
        .next_request()
        .unwrap()
        .1
        .send_response(load::Constant::new(svc1_m, 0));
    with_task(|| {
        assert_ready!(pool);
    });

    // cycling a request should now work
    let mut fut = pool.call(());
    svc1.next_request().unwrap().1.send_response("foo");
    with_task(|| {
        assert_fut_ready!(fut, "foo");
    });
    // and pool should now not be ready (since svc1 isn't ready)
    // it should immediately try to add another service
    // which we give it
    with_task(|| {
        assert_not_ready!(pool);
    });
    let (svc2_m, mut svc2) = mock::pair();
    svc2.allow(1);
    handle
        .next_request()
        .unwrap()
        .1
        .send_response(load::Constant::new(svc2_m, 0));
    // pool is now ready
    // which (because of urgency == 1.0) should immediately cause it to drop a service
    // it'll drop svc1, so it'll still be ready
    with_task(|| {
        assert_ready!(pool);
    });
    // and even with another ready, it won't drop svc2 since its now the only service
    with_task(|| {
        assert_ready!(pool);
    });

    // cycling a request should now work on svc2
    let mut fut = pool.call(());
    svc2.next_request().unwrap().1.send_response("foo");
    with_task(|| {
        assert_fut_ready!(fut, "foo");
    });

    // and again (still svc2)
    svc2.allow(1);
    with_task(|| {
        assert_ready!(pool);
    });
    let mut fut = pool.call(());
    svc2.next_request().unwrap().1.send_response("foo");
    with_task(|| {
        assert_fut_ready!(fut, "foo");
    });
}

#[test]
fn failing_service() {
    // start the pool
    let (mock, mut handle) =
        mock::pair::<(), load::Constant<mock::Mock<(), &'static str>, usize>>();
    let mut pool = Builder::new()
        .urgency(1.0) // so _any_ NotReady will add a service
        .underutilized_below(0.0) // so no Ready will remove a service
        .build(mock, ());
    with_task(|| {
        assert_not_ready!(pool);
    });

    // give the pool a backing service
    let (svc1_m, mut svc1) = mock::pair();
    svc1.allow(1);
    handle
        .next_request()
        .unwrap()
        .1
        .send_response(load::Constant::new(svc1_m, 0));
    with_task(|| {
        assert_ready!(pool);
    });

    // one request-response cycle
    let mut fut = pool.call(());
    svc1.next_request().unwrap().1.send_response("foo");
    with_task(|| {
        assert_fut_ready!(fut, "foo");
    });

    // now make svc1 fail, so it has to be removed
    svc1.send_error("ouch");
    // polling now should recognize the failed service,
    // try to create a new one, and then realize the maker isn't ready
    with_task(|| {
        assert_not_ready!(pool);
    });
    // then we release another service
    let (svc2_m, mut svc2) = mock::pair();
    svc2.allow(1);
    handle
        .next_request()
        .unwrap()
        .1
        .send_response(load::Constant::new(svc2_m, 0));

    // the pool should now be ready again
    with_task(|| {
        assert_ready!(pool);
    });
    // and a cycle should work (and go through svc2)
    let mut fut = pool.call(());
    svc2.next_request().unwrap().1.send_response("bar");
    with_task(|| {
        assert_fut_ready!(fut, "bar");
    });
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::lazy;
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
