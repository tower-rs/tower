use futures::prelude::*;
use tower_ready_cache::{error, ReadyCache};
use tower_test::mock;

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::lazy;
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}

type Req = &'static str;
type Mock = mock::Mock<Req, Req>;

#[test]
fn poll_ready_inner_failure() {
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service0, mut handle0) = mock::pair::<Req, Req>();
    handle0.send_error("doom");
    cache.push(0, service0);

    let (service1, mut handle1) = mock::pair::<Req, Req>();
    handle1.allow(1);
    cache.push(1, service1);

    with_task(|| {
        let error::Failed(key, err) = cache
            .poll_pending()
            .err()
            .expect("poll_ready should fail when exhausted");
        assert_eq!(key, 0);
        assert_eq!(format!("{}", err), "doom");
    });

    assert_eq!(cache.len(), 1);
}

#[test]
fn poll_ready_not_ready() {
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service0, mut handle0) = mock::pair::<Req, Req>();
    handle0.allow(0);
    cache.push(0, service0);

    let (service1, mut handle1) = mock::pair::<Req, Req>();
    handle1.allow(0);
    cache.push(1, service1);

    with_task(|| {
        assert!(cache.poll_pending().expect("must succeed").is_not_ready());
    });

    assert_eq!(cache.ready_len(), 0);
    assert_eq!(cache.pending_len(), 2);
    assert_eq!(cache.len(), 2);
}

#[test]
fn poll_ready_promotes_inner() {
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service0, mut handle0) = mock::pair::<Req, Req>();
    handle0.allow(1);
    cache.push(0, service0);

    let (service1, mut handle1) = mock::pair::<Req, Req>();
    handle1.allow(1);
    cache.push(1, service1);

    assert_eq!(cache.ready_len(), 0);
    assert_eq!(cache.pending_len(), 2);
    assert_eq!(cache.len(), 2);

    with_task(|| {
        assert!(cache.poll_pending().expect("must succeed").is_ready());
    });

    assert_eq!(cache.ready_len(), 2);
    assert_eq!(cache.pending_len(), 0);
    assert_eq!(cache.len(), 2);
}
