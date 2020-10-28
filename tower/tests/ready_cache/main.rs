#![cfg(feature = "ready-cache")]
#[path = "../support.rs"]
mod support;

use tokio_test::{assert_pending, assert_ready, task};
use tower::ready_cache::ReadyCache;
use tower_test::mock;

type Req = &'static str;
type Mock = mock::Mock<Req, Req>;

#[test]
fn poll_ready_inner_failure() {
    let _t = support::trace_init();

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
    let _t = support::trace_init();

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
    let _t = support::trace_init();

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

#[test]
fn evict_ready_then_error() {
    let _t = support::trace_init();

    let mut task = task::spawn(());
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service, mut handle) = mock::pair::<Req, Req>();
    handle.allow(0);
    cache.push(0, service);

    assert_pending!(task.enter(|cx, _| cache.poll_pending(cx)));

    handle.allow(1);
    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();

    handle.send_error("doom");
    assert!(cache.evict(&0));

    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();
}

#[test]
fn evict_pending_then_error() {
    let _t = support::trace_init();

    let mut task = task::spawn(());
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service, mut handle) = mock::pair::<Req, Req>();
    handle.allow(0);
    cache.push(0, service);

    assert_pending!(task.enter(|cx, _| cache.poll_pending(cx)));

    handle.send_error("doom");
    assert!(cache.evict(&0));

    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();
}

#[test]
fn push_then_evict() {
    let _t = support::trace_init();

    let mut task = task::spawn(());
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service, mut handle) = mock::pair::<Req, Req>();
    handle.allow(0);
    cache.push(0, service);
    handle.send_error("doom");
    assert!(cache.evict(&0));

    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();
}

#[test]
fn error_after_promote() {
    let _t = support::trace_init();

    let mut task = task::spawn(());
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service, mut handle) = mock::pair::<Req, Req>();
    handle.allow(0);
    cache.push(0, service);

    assert_pending!(task.enter(|cx, _| cache.poll_pending(cx)));

    handle.allow(1);
    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();

    handle.send_error("doom");
    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();
}

#[test]
fn duplicate_key_by_index() {
    let _t = support::trace_init();

    let mut task = task::spawn(());
    let mut cache = ReadyCache::<usize, Mock, Req>::default();

    let (service0, mut handle0) = mock::pair::<Req, Req>();
    handle0.allow(1);
    cache.push(0, service0);

    let (service1, mut handle1) = mock::pair::<Req, Req>();
    handle1.allow(1);
    // this push should replace the old service (service0)
    cache.push(0, service1);

    // this evict should evict service1
    cache.evict(&0);

    // poll_pending should complete (there are no remaining pending services)
    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();
    // but service 0 should not be ready (1 replaced, 1 evicted)
    assert!(!task.enter(|cx, _| cache.check_ready(cx, &0)).unwrap());

    let (service2, mut handle2) = mock::pair::<Req, Req>();
    handle2.allow(1);
    // this push should ensure replace the evicted service1
    cache.push(0, service2);

    // there should be no more pending
    assert_ready!(task.enter(|cx, _| cache.poll_pending(cx))).unwrap();
    // _and_ service 0 should now be callable
    assert!(task.enter(|cx, _| cache.check_ready(cx, &0)).unwrap());
}
