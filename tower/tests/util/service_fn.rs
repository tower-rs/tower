use futures_util::future::ready;
use tower_service::Service;
use tower_util::service_fn;

#[tokio::test]
async fn simple() {
    let mut add_one = service_fn(|req| ready(Ok::<_, ()>(req + 1)));
    let answer = add_one.call(1).await.unwrap();
    assert_eq!(answer, 2);
}
