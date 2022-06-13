use futures_util::future::ready;
use tower::util::{service_fn, ServiceExt};
use tower_service::Call;

#[tokio::test(flavor = "current_thread")]
async fn simple() {
    let _t = super::support::trace_init();

    let mut add_one = service_fn(|req| ready(Ok::<_, ()>(req + 1)));
    let call = add_one.ready().await.unwrap();
    let answer = call.call(1).await.unwrap();
    assert_eq!(answer, 2);
}
