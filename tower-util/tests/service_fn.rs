use futures_util::future::ready;
use tokio_test::block_on;
use tower_service::Service;
use tower_util::service_fn;

#[test]
fn simple() {
    let mut add_one = service_fn(|req| ready(Ok::<_, ()>(req + 1)));
    let answer = block_on(add_one.call(1)).unwrap();
    assert_eq!(answer, 2);
}
