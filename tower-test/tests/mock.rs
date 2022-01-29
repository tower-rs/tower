use tokio_test::{assert_pending, assert_ready};
use tower_test::{assert_request_eq, mock};

#[tokio::test(flavor = "current_thread")]
async fn single_request_ready() {
    let (mut service, mut handle) = mock::spawn();

    assert_pending!(handle.poll_request());

    let token = assert_ready!(service.poll_ready()).unwrap();

    let response = service.call(token, "hello");

    assert_request_eq!(handle, "hello").send_response("world");

    assert_eq!(response.await.unwrap(), "world");
}

#[tokio::test(flavor = "current_thread")]
async fn backpressure() {
    let (mut service, mut handle) = mock::spawn::<&'static str, ()>();

    handle.allow(0);

    assert_pending!(service.poll_ready());
}
