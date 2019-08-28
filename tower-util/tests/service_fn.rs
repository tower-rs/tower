use futures::executor::block_on;
use futures::future;
use tower_service::Service;
use tower_util::service_fn;

#[test]
fn simple() {
    block_on(async {
        let mut add_one = service_fn(|req| future::ok::<_, ()>(req + 1));
        let answer = add_one.call(1).await.unwrap();
        assert_eq!(answer, 2);
    });
}
