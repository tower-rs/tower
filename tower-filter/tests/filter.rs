extern crate futures;
extern crate tower_mock;
extern crate tower_filter;
extern crate tower_service;

use futures::*;
use tower_filter::*;
use tower_service::*;

use std::thread;
use std::sync::mpsc;

#[test]
fn passthrough_sync() {
    let (mut service, mut handle) =
        new_service(10, |_| Ok::<_, ()>(()));

    let th = thread::spawn(move || {
        // Receive the requests and respond
        for i in 0..10 {
            let expect = format!("ping-{}", i);
            let actual = handle.next_request().unwrap();

            assert_eq!(actual.as_str(), expect.as_str());

            actual.respond(format!("pong-{}", i));
        }
    });

    let mut responses = vec![];

    for i in 0..10 {
        let request = format!("ping-{}", i);
        let exchange = service.call(request)
            .and_then(move |response| {
                let expect = format!("pong-{}", i);
                assert_eq!(response.as_str(), expect.as_str());

                Ok(())
            });

        responses.push(exchange);
    }

    future::join_all(responses).wait().unwrap();
    th.join().unwrap();
}

#[test]
fn rejected_sync() {
    let (mut service, _handle) =
        new_service(10, |_| Err::<(), _>(()));

    let response = service.call("hello".into()).wait();
    assert!(response.is_err());
}

#[test]
fn saturate() {
    use futures::stream::FuturesUnordered;

    let (mut service, mut handle) =
        new_service(1, |_| Ok::<_, ()>(()));

    with_task(|| {
        // First request is ready
        assert!(service.poll_ready().unwrap().is_ready());
    });

    let mut r1 = service.call("one".into());

    with_task(|| {
        // Second request is not ready
        assert!(service.poll_ready().unwrap().is_not_ready());
    });

    let mut futs = FuturesUnordered::new();
    futs.push(service.ready());

    let (tx, rx) = mpsc::channel();

    // Complete the request in another thread
    let th1 = thread::spawn(move || {
        with_task(|| {
            assert!(r1.poll().unwrap().is_not_ready());

            tx.send(()).unwrap();

            let response = r1.wait().unwrap();
            assert_eq!(response.as_str(), "resp-one");
        });
    });

    rx.recv().unwrap();

    // The service should be ready
    let mut service = with_task(|| {
        match futs.poll().unwrap() {
            Async::Ready(Some(s)) => s,
            Async::Ready(None) => panic!("None"),
            Async::NotReady => panic!("NotReady"),
        }
    });

    let r2 = service.call("two".into());

    let th2 = thread::spawn(move || {
        let response = r2.wait().unwrap();
        assert_eq!(response.as_str(), "resp-two");
    });

    let request = handle.next_request().unwrap();
    assert_eq!("one", request.as_str());
    request.respond("resp-one".into());

    let request = handle.next_request().unwrap();
    assert_eq!("two", request.as_str());
    request.respond("resp-two".into());

    th1.join().unwrap();
    th2.join().unwrap();
}

type Mock = tower_mock::Mock<String, String, ()>;
type Handle = tower_mock::Handle<String, String, ()>;

fn new_service<F, U>(max: usize, f: F) -> (Filter<Mock, F>, Handle)
where F: Fn(&String) -> U,
      U: IntoFuture<Item = ()>
{
    let (service, handle) = Mock::new();
    let service = Filter::new(service, f, max);
    (service, handle)
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::{Future, lazy};
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
