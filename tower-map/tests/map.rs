use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn map_service() {
    let (service1, mut handle): (_, Handle) = mock::pair();

    let map_request = |r1| format!("{} world!", r1);
    let mut service2 = tower_map::Map::new(map_request, service1);

    assert!(service2.poll_ready().unwrap().is_ready());

    let _ = service2.call("hello");
    assert_request_eq!(handle, "hello world!");
}

type Handle = mock::Handle<String, &'static str>;
