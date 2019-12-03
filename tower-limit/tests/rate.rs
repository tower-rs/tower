use futures_util::pin_mut;
use tokio_test::{assert_pending, assert_ready, assert_ready_ok};
use tower_limit::rate::*;
use tower_service::*;
use tower_test::{assert_request_eq, mock};

use std::future::Future;
use std::time::Duration;

#[tokio::test]
async fn reaching_capacity() {
    let (service, handle) = new_service(Rate::new(1, from_millis(100)));
    pin_mut!(handle);
    let mut svc = mock::spawn(service);

    assert_ready_ok!(svc.poll_ready());

    let response = svc.call("hello");

    assert_request_eq!(handle, "hello").send_response("world");

    // let response = response.await.unwrap();
    // assert_eq!(response, "world");
    //     assert_pending!(task.enter(|cx| service.poll_ready(cx)));
    //     assert_pending!(task.enter(|cx| handle.as_mut().poll_request(cx)));

    //     time.advance(Duration::from_millis(100));

    //     assert_ready_ok!(task.enter(|cx| service.poll_ready(cx)));

    //     // Send a second request
    //     let response = service.call("two");
    //     pin_mut!(response);

    //     assert_request_eq!(handle, "two").send_response("done");
    //     assert_ready_ok!(task.enter(|cx| response.poll(cx)), "done");
}

type Mock = mock::Mock<&'static str, &'static str>;
type Handle = mock::Handle<&'static str, &'static str>;

fn new_service(rate: Rate) -> (RateLimit<Mock>, Handle) {
    let (service, handle) = mock::pair();
    let service = RateLimit::new(service, rate);
    (service, handle)
}

fn from_millis(n: u64) -> Duration {
    Duration::from_millis(n)
}
