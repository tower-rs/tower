#![cfg(feature = "spawn-ready")]
#[path = "../support.rs"]
mod support;

use tokio::time;
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok};
use tower::spawn_ready::{SpawnReady, SpawnReadyLayer};
use tower_test::mock;

#[tokio::test(flavor = "current_thread")]
async fn when_inner_is_not_ready() {
    time::pause();

    let _t = support::trace_init();

    let layer = SpawnReadyLayer::new();
    let (mut service, mut handle) = mock::spawn_layer::<(), (), _>(layer);

    // Make the service NotReady
    handle.allow(0);

    assert_pending!(service.poll_ready());

    // Make the service is Ready
    handle.allow(1);
    time::sleep(time::Duration::from_millis(100)).await;
    assert_ready_ok!(service.poll_ready());
}

#[tokio::test(flavor = "current_thread")]
async fn when_inner_fails() {
    let _t = support::trace_init();

    let layer = SpawnReadyLayer::new();
    let (mut service, mut handle) = mock::spawn_layer::<(), (), _>(layer);

    // Make the service NotReady
    handle.allow(0);
    handle.send_error("foobar");

    assert_eq!(
        assert_ready_err!(service.poll_ready()).to_string(),
        "foobar"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn propagates_trace_spans() {
    use tower::util::ServiceExt;
    use tracing::Instrument;

    let _t = support::trace_init();

    let span = tracing::info_span!("my_span");

    let service = support::AssertSpanSvc::new(span.clone());
    let service = SpawnReady::new(service);
    let result = tokio::spawn(service.oneshot(()).instrument(span));

    result.await.expect("service panicked").expect("failed");
}
