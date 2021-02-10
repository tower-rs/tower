#![cfg(feature = "spawn-ready")]
#[path = "../support.rs"]
mod support;

use tokio::task;
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok};
use tower::spawn_ready::SpawnReadyLayer;
use tower_test::mock;

#[tokio::test(flavor = "current_thread")]
async fn when_inner_is_not_ready() {
    let _t = support::trace_init();

    let layer = SpawnReadyLayer::new();
    let (mut service, mut handle) = mock::spawn_layer::<(), (), _>(layer);

    // Make the service NotReady
    handle.allow(0);

    assert_pending!(service.poll_ready());

    // Make the service is Ready
    handle.allow(1);
    // Yield to the executor to ensure that the background task can run.
    task::yield_now().await;
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
