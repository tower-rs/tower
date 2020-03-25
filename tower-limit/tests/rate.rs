use std::time::Duration;
use tokio::time;
use tokio_test::{assert_pending, assert_ready, assert_ready_ok};
use tower_limit::rate::RateLimitLayer;
use tower_load::Load;
use tower_test::{assert_request_eq, mock};

#[tokio::test]
async fn reaching_capacity() {
    time::pause();

    let rate_limit = RateLimitLayer::new(1, Duration::from_millis(100));

    let (mut service, mut handle) = mock::spawn_layer(rate_limit);

    assert_ready_ok!(service.poll_ready());

    let response = service.call("hello");
    let load1 = service.get_ref().load();

    assert_request_eq!(handle, "hello").send_response("world");

    assert_eq!(response.await.unwrap(), "world");
    assert_pending!(service.poll_ready());
    let load2 = service.get_ref().load();
    assert!(load2 >= load1);

    assert_pending!(handle.poll_request());

    time::advance(Duration::from_millis(101)).await;

    assert_ready_ok!(service.poll_ready());
    let load3 = service.get_ref().load();
    assert!(load3 < load2);

    let response = service.call("two");
    let load4 = service.get_ref().load();
    assert!(load4 >= load3);

    assert_request_eq!(handle, "two").send_response("done");

    assert_eq!(response.await.unwrap(), "done");
}
