use futures_util::pin_mut;
use tokio_test::{assert_pending, assert_ready, assert_ready_ok, clock, task::MockTask};
use tower_limit::rate::*;
use tower_service::*;
use tower_test::{assert_request_eq, mock};

use std::future::Future;
use std::time::Duration;

#[test]
fn reaching_capacity() {
    clock::mock(|time| {
        let mut task = MockTask::new();

        let (mut service, handle) = new_service(Rate::new(1, from_millis(100)));
        pin_mut!(handle);

        assert_ready_ok!(task.enter(|cx| service.poll_ready(cx)));
        let response = service.call("hello");
        pin_mut!(response);

        assert_request_eq!(handle, "hello").send_response("world");
        assert_ready_ok!(task.enter(|cx| response.poll(cx)), "world");

        assert_pending!(task.enter(|cx| service.poll_ready(cx)));
        assert_pending!(task.enter(|cx| handle.as_mut().poll_request(cx)));

        time.advance(Duration::from_millis(100));

        assert_ready_ok!(task.enter(|cx| service.poll_ready(cx)));

        // Send a second request
        let response = service.call("two");
        pin_mut!(response);

        assert_request_eq!(handle, "two").send_response("done");
        assert_ready_ok!(task.enter(|cx| response.poll(cx)), "done");
    });
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
