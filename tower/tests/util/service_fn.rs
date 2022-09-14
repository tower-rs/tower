use futures_util::future::ready;
use tower_service::Service;

#[tokio::test(flavor = "current_thread")]
async fn simple() {
    let _t = super::support::trace_init();

    let mut add_one = |req| ready(Ok::<_, ()>(req + 1));
    let answer = add_one.call(1).await.unwrap();
    assert_eq!(answer, 2);
}
