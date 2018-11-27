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

type Mock = tower_mock::Mock<&'static str, &'static str, ()>;
type Handle = tower_mock::Handle<&'static str, &'static str, ()>;

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
    let service = Buffer::with_executor(service, &Exec).unwrap();
    (service, handle)
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::{Future, lazy};
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
