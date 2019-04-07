use futures::*;
use std::thread;
use tower_filter::{error::Error, Filter};
use tower_service::Service;

#[test]
fn passthrough_sync() {
    let (mut service, mut handle) = new_service(|_| Ok(()));

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
        assert!(service.poll_ready().unwrap().is_ready());
        let exchange = service.call(request).and_then(move |response| {
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
    let (mut service, _handle) = new_service(|_| Err(Error::rejected()));

    let response = service.call("hello".into()).wait();
    assert!(response.is_err());
}

type Mock = tower_mock::Mock<String, String>;
type Handle = tower_mock::Handle<String, String>;

fn new_service<F, U>(f: F) -> (Filter<Mock, F>, Handle)
where
    F: Fn(&String) -> U,
    U: IntoFuture<Item = (), Error = Error>,
{
    let (service, handle) = Mock::new();
    let service = Filter::new(service, f);
    (service, handle)
}
