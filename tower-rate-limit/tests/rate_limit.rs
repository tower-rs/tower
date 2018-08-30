extern crate futures;
extern crate tower_mock;
extern crate tower_rate_limit;
extern crate tower_service;
extern crate tokio_timer;
extern crate tokio;

use futures::future;
use tower_rate_limit::*;
use tower_service::*;

use std::time::{Duration, Instant};

#[test]
fn reaching_capacity() {
    let mut rt = tokio::runtime::current_thread::Runtime::new()
        .unwrap();
    let (mut service, mut handle) =
        new_service(Rate::new(1, from_millis(100)));

    let response = service.call("hello");

    let request = handle.next_request().unwrap();
    assert_eq!(*request, "hello");
    request.respond("world");

    let response = rt.block_on(response);
    assert_eq!(response.unwrap(), "world");

    // Sending another request is rejected
    let response = service.call("no");

    let poll_request = rt.block_on(future::lazy(|| handle.poll_request()));
    assert!(poll_request.unwrap().is_not_ready());

    let response = rt.block_on(response);
    assert!(response.is_err());

    let poll_request = rt.block_on(future::lazy(|| handle.poll_request()));
    assert!(poll_request.unwrap().is_not_ready());

    // Unlike `thread::sleep`, this advances the timer.
    rt.block_on(tokio_timer::Delay::new(Instant::now() + Duration::from_millis(100)))
        .unwrap();

    let poll_ready = rt.block_on(future::lazy(|| service.poll_ready()));
    assert!(poll_ready.unwrap().is_ready());

    // Send a second request
    let response = service.call("two");

    let request = handle.next_request().unwrap();
    assert_eq!(*request, "two");
    request.respond("done");

    let response = rt.block_on(response);
    assert_eq!(response.unwrap(), "done");
}

type Mock = tower_mock::Mock<&'static str, &'static str, ()>;
type Handle = tower_mock::Handle<&'static str, &'static str, ()>;

fn new_service(rate: Rate) -> (RateLimit<Mock>, Handle) {
    let (service, handle) = Mock::new();
    let service = RateLimit::new(service, rate);
    (service, handle)
}

fn from_millis(n: u64) -> Duration {
    Duration::from_millis(n)
}
