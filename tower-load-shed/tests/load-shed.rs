use futures::Future;
use tower_load_shed::{self, LoadShed};
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn when_ready() {
    let (mut service, mut handle) = new_service();

    with_task(|| {
        assert!(
            service.poll_ready().unwrap().is_ready(),
            "overload always reports ready",
        );
    });

    let response = service.call("hello");

    assert_request_eq!(handle, "hello").send_response("world");
    assert_eq!(response.wait().unwrap(), "world");
}

#[test]
fn when_not_ready() {
    let (mut service, mut handle) = new_service();

    handle.allow(0);

    with_task(|| {
        assert!(
            service.poll_ready().unwrap().is_ready(),
            "overload always reports ready",
        );
    });

    let fut = service.call("hello");

    let err = fut.wait().unwrap_err();
    assert!(err.is::<tower_load_shed::error::Overloaded>());
}

type Mock = mock::Mock<&'static str, &'static str>;
type Handle = mock::Handle<&'static str, &'static str>;

fn new_service() -> (LoadShed<Mock>, Handle) {
    let (service, handle) = mock::pair();
    let service = LoadShed::new(service);
    (service, handle)
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::{lazy, Future};
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
