use futures_util::future::ready;
use tower::util::{service_fn, UnsyncBoxCloneService};
use tower_service::Service;

#[tokio::test(flavor = "current_thread")]
async fn simple() {
    let _t = super::support::trace_init();

    let mut add_one = service_fn(|req| ready(Ok::<_, ()>(req + 1)));
    let answer = add_one.call(1).await.unwrap();
    assert_eq!(answer, 2);
}

#[tokio::test(flavor = "current_thread")]
async fn boxed_clone() {
    let _t = super::support::trace_init();
    let x = std::rc::Rc::new(1);
    let mut add_one = service_fn(|req| ready(Ok::<_, ()>(req + 1)));
    let mut cloned = UnsyncBoxCloneService::new(add_one);
    let answer = cloned.call(1).await.unwrap();
    assert_eq!(answer, 2);
    let answer = add_one.call(1).await.unwrap();
    assert_eq!(answer, 2);
}
