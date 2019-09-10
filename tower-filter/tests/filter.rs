use futures_util::{future::poll_fn, pin_mut};
use std::{future::Future, thread};
use tokio_test::{assert_ready, assert_ready_err, task};
use tower_filter::{error::Error, Filter};
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[tokio::test]
async fn passthrough_sync() {
    let (mut service, handle) = new_service(|_| async { Ok(()) });

    let th = thread::spawn(move || {
        // Receive the requests and respond
        pin_mut!(handle);
        for i in 0..10 {
            assert_request_eq!(handle, format!("ping-{}", i)).send_response(format!("pong-{}", i));
        }
    });

    let mut responses = vec![];

    for i in 0usize..10 {
        let request = format!("ping-{}", i);
        poll_fn(|cx| service.poll_ready(cx)).await.unwrap();
        let exchange = service.call(request);
        let exchange = async move {
            let response = exchange.await.unwrap();
            let expect = format!("pong-{}", i);
            assert_eq!(response.as_str(), expect.as_str());
        };

        responses.push(exchange);
    }

    futures_util::future::join_all(responses).await;
    th.join().unwrap();
}

#[test]
fn rejected_sync() {
    task::mock(|cx| {
        let (mut service, _handle) = new_service(|_| async { Err(Error::rejected()) });

        let fut = service.call("hello".into());
        pin_mut!(fut);
        assert_ready_err!(fut.poll(cx));
    });
}

type Mock = mock::Mock<String, String>;
type Handle = mock::Handle<String, String>;

fn new_service<F, U>(f: F) -> (Filter<Mock, F>, Handle)
where
    F: Fn(&String) -> U,
    U: Future<Output = Result<(), Error>>,
{
    let (service, handle) = mock::pair();
    let service = Filter::new(service, f);
    (service, handle)
}
