extern crate futures;
extern crate tokio;
extern crate tokio_timer;
extern crate tower_limit;
extern crate tower_service;
#[macro_use]
extern crate tower_test;

use futures::future;
use tower_limit::rate::*;
use tower_service::*;
use tower_test::mock;

use std::time::{Duration, Instant};

macro_rules! assert_ready {
    ($e:expr) => {{
        use futures::Async::*;
        match $e {
            Ok(Ready(v)) => v,
            Ok(NotReady) => panic!("not ready"),
            Err(e) => panic!("err = {:?}", e),
        }
    }};
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        use futures::Async::*;
        match $e {
            Ok(NotReady) => {}
            r => panic!("unexpected poll status = {:?}", r),
        }
    }};
}

#[test]
fn reaching_capacity() {
    let mut rt = tokio::runtime::current_thread::Runtime::new().unwrap();
    let (mut service, mut handle) = new_service(Rate::new(1, from_millis(100)));

    assert_ready!(service.poll_ready());
    let response = service.call("hello");

    assert_request_eq!(handle, "hello").send_response("world");

    let response = rt.block_on(response);
    assert_eq!(response.unwrap(), "world");

    rt.block_on(future::lazy(|| {
        assert_not_ready!(service.poll_ready());
        Ok::<_, ()>(())
    }))
    .unwrap();

    let poll_request = rt.block_on(future::lazy(|| handle.poll_request()));
    assert!(poll_request.unwrap().is_not_ready());

    // Unlike `thread::sleep`, this advances the timer.
    rt.block_on(tokio_timer::Delay::new(
        Instant::now() + Duration::from_millis(100),
    ))
    .unwrap();

    let poll_ready = rt.block_on(future::lazy(|| service.poll_ready()));
    assert_ready!(poll_ready);

    // Send a second request
    let response = service.call("two");

    assert_request_eq!(handle, "two").send_response("done");

    let response = rt.block_on(response);
    assert_eq!(response.unwrap(), "done");
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
