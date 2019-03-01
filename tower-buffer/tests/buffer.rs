extern crate futures;
extern crate tower_buffer;
extern crate tower_mock;
extern crate tower_service;

use futures::prelude::*;
use tower_buffer::*;
use tower_service::*;

use std::thread;

#[test]
fn req_and_res() {
    let (mut service, mut handle) = new_service();

    let response = service.call("hello");

    let request = handle.next_request().unwrap();
    assert_eq!(*request, "hello");
    request.respond("world");

    assert_eq!(response.wait().unwrap(), "world");
}

#[test]
fn clears_canceled_requests() {
    let (mut service, mut handle) = new_service();

    handle.allow(1);

    let res1 = service.call("hello");

    let req1 = handle.next_request().unwrap();
    assert_eq!(*req1, "hello");

    // don't respond yet, new requests will get buffered

    let res2 = service.call("hello2");
    with_task(|| {
        assert!(handle.poll_request().unwrap().is_not_ready());
    });

    let res3 = service.call("hello3");

    drop(res2);

    req1.respond("world");
    assert_eq!(res1.wait().unwrap(), "world");

    // res2 was dropped, so it should have been canceled in the buffer
    handle.allow(1);

    let req3 = handle.next_request().unwrap();
    assert_eq!(*req3, "hello3");
    req3.respond("world3");
    assert_eq!(res3.wait().unwrap(), "world3");
}

#[test]
fn when_inner_is_not_ready() {
    let (mut service, mut handle) = new_service();

    // Make the service NotReady
    handle.allow(0);

    let mut res1 = service.call("hello");

    // Allow the Buffer's executor to do work
    ::std::thread::sleep(::std::time::Duration::from_millis(100));
    with_task(|| {
        assert!(res1.poll().expect("res1.poll").is_not_ready());
        assert!(handle.poll_request().expect("poll_request").is_not_ready());
    });

    handle.allow(1);

    let req1 = handle.next_request().expect("next_request1");
    assert_eq!(*req1, "hello");
    req1.respond("world");

    assert_eq!(res1.wait().expect("res1.wait"), "world");
}

#[test]
fn when_inner_fails() {
    use std::error::Error as StdError;

    let (mut service, mut handle) = new_service();

    // Make the service NotReady
    handle.allow(0);
    handle.error("foobar");

    let mut res1 = service.call("hello");

    // Allow the Buffer's executor to do work
    ::std::thread::sleep(::std::time::Duration::from_millis(100));
    with_task(|| {
        let e = res1.poll().unwrap_err();
        if let Some(e) = e.downcast_ref::<error::ServiceError>() {
            assert!(format!("{}", e).contains("poll_ready"));

            let e = e.source().unwrap();

            assert_eq!(e.to_string(), "foobar");
        } else {
            panic!("unexpected error type: {:?}", e);
        }
    });
}

type Mock = tower_mock::Mock<&'static str, &'static str>;
type Handle = tower_mock::Handle<&'static str, &'static str>;

struct Exec;

impl<F> futures::future::Executor<F> for Exec
where
    F: Future<Item = (), Error = ()> + Send + 'static,
{
    fn execute(&self, fut: F) -> Result<(), futures::future::ExecuteError<F>> {
        thread::spawn(move || {
            fut.wait().unwrap();
        });
        Ok(())
    }
}

fn new_service() -> (Buffer<Mock, &'static str>, Handle) {
    let (service, handle) = Mock::new();
    // bound is >0 here because clears_canceled_requests needs multiple outstanding requests
    let service = Buffer::with_executor(service, 10, &Exec).unwrap();
    (service, handle)
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::{lazy, Future};
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
