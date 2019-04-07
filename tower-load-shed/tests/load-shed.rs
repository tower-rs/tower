use futures::Future;
use tower_load_shed::{self, LoadShed};
use tower_service::Service;

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

    let request = handle.next_request().unwrap();
    assert_eq!(*request, "hello");
    request.respond("world");

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

type Mock = tower_mock::Mock<&'static str, &'static str>;
type Handle = tower_mock::Handle<&'static str, &'static str>;

fn new_service() -> (LoadShed<Mock>, Handle) {
    let (service, handle) = Mock::new();
    let service = LoadShed::new(service);
    (service, handle)
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::{lazy, Future};
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
