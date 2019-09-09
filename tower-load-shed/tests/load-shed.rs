use futures_util::pin_mut;
use std::future::Future;
use tokio_test::{assert_ready_err, assert_ready_ok, task::mock};
use tower_load_shed::{self, LoadShed};
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn when_ready() {
    mock(|cx| {
        let (mut service, handle) = new_service();
        pin_mut!(handle);

        assert_ready_ok!(service.poll_ready(cx), "overload always reports ready");

        let response = service.call("hello");
        pin_mut!(response);

        assert_request_eq!(handle, "hello").send_response("world");
        assert_eq!(assert_ready_ok!(response.poll(cx)), "world");
    });
}

#[test]
fn when_not_ready() {
    mock(|cx| {
        let (mut service, handle) = new_service();
        pin_mut!(handle);

        handle.allow(0);

        assert_ready_ok!(service.poll_ready(cx), "overload always reports ready");

        let fut = service.call("hello");
        pin_mut!(fut);

        let err = assert_ready_err!(fut.poll(cx));
        assert!(err.is::<tower_load_shed::error::Overloaded>());
    });
}

type Mock = mock::Mock<&'static str, &'static str>;
type Handle = mock::Handle<&'static str, &'static str>;

fn new_service() -> (LoadShed<Mock>, Handle) {
    let (service, handle) = mock::pair();
    let service = LoadShed::new(service);
    (service, handle)
}
