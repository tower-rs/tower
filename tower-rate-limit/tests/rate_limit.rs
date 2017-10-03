extern crate futures;
extern crate tower;
extern crate tower_mock;
extern crate tower_rate_limit;
extern crate tokio_timer;

use futures::prelude::*;
use tower::*;
use tower_rate_limit::*;

use std::time::Duration;
use std::thread;

#[test]
fn reaching_capacity() {
    let (mut service, mut handle) =
        new_service(Rate::new(1, from_millis(100)));

    let response = service.call("hello");

    let request = handle.next_request().unwrap();
    assert_eq!(*request, "hello");
    request.respond("world");

    assert_eq!(response.wait().unwrap(), "world");

    // Sending another request is rejected
    let response = service.call("no");
    with_task(|| {
        assert!(handle.poll_request().unwrap().is_not_ready());
    });

    assert!(response.wait().is_err());

    with_task(|| {
        assert!(service.poll_ready().unwrap().is_not_ready());
    });

    thread::sleep(Duration::from_millis(100));

    with_task(|| {
        assert!(service.poll_ready().unwrap().is_ready());
    });

    // Send a second request
    let response = service.call("two");

    let request = handle.next_request().unwrap();
    assert_eq!(*request, "two");
    request.respond("done");

    assert_eq!(response.wait().unwrap(), "done");
}

type Mock = tower_mock::Mock<&'static str, &'static str, ()>;
type Handle = tower_mock::Handle<&'static str, &'static str, ()>;

fn new_service(rate: Rate) -> (RateLimit<Mock>, Handle) {
    let timer = tokio_timer::wheel()
        .tick_duration(Duration::from_millis(1))
        .build();

    let (service, handle) = Mock::new();
    let service = RateLimit::new(service, rate, timer);
    (service, handle)
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::{Future, lazy};
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}

fn from_millis(n: u64) -> Duration {
    Duration::from_millis(n)
}
