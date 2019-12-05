use tokio_test::{assert_pending, assert_ready, task};
use tower_ready_cache::ReadyCache;
use tower_test::mock;

type Req = &'static str;
type Mock = mock::Mock<Req, Req>;

#[test]
fn poll_ready_inner_failure() {
    let mut task = task::spawn(());
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service0, mut handle0) = mock::pair::<Req, Req>();
    handle0.send_error("doom");
    cache.push(0, service0);

    let (service1, mut handle1) = mock::pair::<Req, Req>();
    handle1.allow(1);
    cache.push(1, service1);

    let failed = assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap_err();

    assert_eq!(failed.0, 0);
    assert_eq!(format!("{}", failed.1), "doom");

    assert_eq!(cache.len(), 1);
}

#[test]
fn poll_ready_not_ready() {
    let mut task = task::spawn(());
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service0, mut handle0) = mock::pair::<Req, Req>();
    handle0.allow(0);
    cache.push(0, service0);

    let (service1, mut handle1) = mock::pair::<Req, Req>();
    handle1.allow(0);
    cache.push(1, service1);

    assert_pending!(task.enter(|cx, _| cache.poll_pending(cx)));

    assert_eq!(cache.ready_len(), 0);
    assert_eq!(cache.pending_len(), 2);
    assert_eq!(cache.len(), 2);
}

#[test]
fn poll_ready_promotes_inner() {
    let mut task = task::spawn(());
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

    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();

    assert_eq!(cache.ready_len(), 2);
    assert_eq!(cache.pending_len(), 0);
    assert_eq!(cache.len(), 2);
}
