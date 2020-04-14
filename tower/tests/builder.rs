#![cfg(all(feature = "buffer", feature = "limit", feature = "retry"))]

use futures_util::{future::Ready, pin_mut};
use std::time::Duration;
use tower::builder::ServiceBuilder;
use tower::retry::Policy;
use tower::util::ServiceExt;
use tower_service::*;
use tower_test::{assert_request_eq, mock};

#[tokio::test]
async fn builder_service() {
    let (service, handle) = mock::pair();
    pin_mut!(handle);

    let policy = MockPolicy::<&'static str, bool>::default();
    let mut client = ServiceBuilder::new()
        .buffer(5)
        .concurrency_limit(5)
        .rate_limit(5, Duration::from_secs(5))
        .retry(policy)
        .map_response(|r: &'static str| if r == "world" { true } else { false })
        .map_request(|r: &'static str| if r == "hello" { true } else { false })
        .service(service);

    // allow a request through
    handle.allow(1);

    let fut = client.ready_and().await.unwrap().call("hello");
    assert_request_eq!(handle, true).send_response("world");
    assert_eq!(fut.await.unwrap(), true);
}

#[derive(Debug, Clone, Default)]
struct MockPolicy<Req, Res> {
    _pd: std::marker::PhantomData<(Req, Res)>,
}

impl<Req, Res, E> Policy<Req, Res, E> for MockPolicy<Req, Res>
where
    Req: Clone,
    E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    type Future = Ready<Self>;

    fn retry(&self, _req: &Req, _result: Result<&Res, &E>) -> Option<Self::Future> {
        None
    }

    fn clone_request(&self, req: &Req) -> Option<Req> {
        Some(req.clone())
    }
}
