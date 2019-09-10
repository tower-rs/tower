use std::time::Duration;
use futures_util::{pin_mut, future::{Ready, poll_fn}};
use tower::builder::ServiceBuilder;
use tower::util::ServiceExt;
use tower_buffer::BufferLayer;
use tower_limit::{concurrency::ConcurrencyLimitLayer, rate::RateLimitLayer};
use tower_retry::{Policy, RetryLayer};
use tower_service::*;
use tower_test::{mock};

#[tokio::test]
async fn builder_service() {
        let (service, handle) = mock::pair();
        pin_mut!(handle);

        let policy = MockPolicy;
        let client = ServiceBuilder::new()
            .layer(BufferLayer::new(5))
            .layer(ConcurrencyLimitLayer::new(5))
            .layer(RateLimitLayer::new(5, Duration::from_secs(1)))
            .layer(RetryLayer::new(policy))
            .layer(BufferLayer::new(5))
            .service(service);

        // allow a request through
        handle.allow(1);

        let mut client = client.ready().await.unwrap();
        let fut = client.call("hello");
        let (request, rsp) = poll_fn(|cx| handle.as_mut().poll_request(cx)).await.unwrap();
        assert_eq!(request, "hello");
        rsp.send_response("world");
        assert_eq!(fut.await.unwrap(), "world");
}

#[derive(Debug, Clone)]
struct MockPolicy;

impl<E> Policy<&'static str, &'static str, E> for MockPolicy
where
    E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    type Future = Ready<Self>;

    fn retry(&self, _req: &&'static str, _result: Result<&&'static str, &E>) -> Option<Self::Future> {
        None
    }

    fn clone_request(&self, req: &&'static str) -> Option<&'static str> {
        Some(req.clone())
    }
}
