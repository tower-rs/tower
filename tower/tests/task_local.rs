#![cfg(all(feature = "task-local", feature = "util"))]

use futures::pin_mut;
use tower::{util::ServiceExt, Service as _};
use tower_test::{assert_request_eq, mock};

mod support;

tokio::task_local! {
    static NUM: i32;
}

#[tokio::test]
async fn set_task_local() {
    let _t = support::trace_init();

    let (service, handle) = mock::pair();
    pin_mut!(handle);

    let mut client = service
        .map_request(|()| {
            assert_eq!(NUM.get(), 9000);
        })
        .map_response(|()| {
            assert_eq!(NUM.get(), 9000);
        })
        .set_task_local(&NUM, 9000);

    // allow a request through
    handle.allow(1);

    let ready = client.ready().await.unwrap();
    let fut = ready.call(());
    assert_request_eq!(handle, ()).send_response(());
    fut.await.unwrap();
}
