use tokio::time::{self, Duration};
use tokio_test::{assert_err, assert_ready_ok};
use tower_test::{
    assert_request_eq,
    mock::{self, Spawn},
};
use tower_timeout::{Timeout, TimeoutLayer};

type Err = Box<dyn std::error::Error + Send + Sync + 'static>;

#[tokio::test]
async fn timeout() -> Result<(), Err> {
    time::pause();

    let timeout = TimeoutLayer::new(Duration::from_millis(100));
    let (mut service, mut handle): (Spawn<Timeout<_>>, _) = mock::spawn_layer(timeout);

    assert_ready_ok!(service.poll_ready());

    let response = service.call("hello");
    assert_request_eq!(handle, "hello").send_response("world");
    assert_eq!(response.await?, "world");

    assert_ready_ok!(service.poll_ready());
    let response = service.call("two");
    time::advance(Duration::from_millis(200)).await;
    assert_err!(response.await);

    Ok(())
}
