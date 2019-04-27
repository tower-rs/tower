use tower_map::MapRequest;
use tower_test::mock;
use tower_service::Service;

type Mock = mock::Mock<String, String>;
type Handle = mock::Handle<String, String>;

#[test]
fn convert_types(){
  let (service, handle) = mock::pair::<Mock, Handle>();
  let mut a = MapRequest::new(|r: &'static str| r.len(), service);

  // TODO: why is poll_ready not here?
  assert!(a.poll_ready().unwrap().is_ready());

  let response = a.call("test").wait();
  assert_eq!(response, 4);
}
