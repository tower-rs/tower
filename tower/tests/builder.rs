use futures::{
    future::{self, FutureResult},
    prelude::*,
};
use std::time::Duration;
use tower::builder::ServiceBuilder;
use tower_buffer::BufferLayer;
use tower_limit::{concurrency::ConcurrencyLimitLayer, rate::RateLimitLayer};
use tower_retry::{Policy, RetryLayer};
use tower_service::*;
use void::Void;

#[test]
fn builder_service() {
    tokio::run(future::lazy(|| {
        let policy = MockPolicy;
        let mut client = ServiceBuilder::new()
            .layer(BufferLayer::new(5))
            .layer(ConcurrencyLimitLayer::new(5))
            .layer(RateLimitLayer::new(5, Duration::from_secs(1)))
            .layer(RetryLayer::new(policy))
            .layer(BufferLayer::new(5))
            .service(MockSvc);

        client.poll_ready().unwrap();
        client
            .call(Request)
            .map(|_| ())
            .map_err(|_| panic!("this is bad"))
    }));
}

#[derive(Debug, Clone)]
struct Request;
#[derive(Debug, Clone)]
struct Response;
#[derive(Debug)]
struct MockSvc;
impl Service<Request> for MockSvc {
    type Response = Response;
    type Error = Void;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _: Request) -> Self::Future {
        future::ok(Response)
    }
}

#[derive(Debug, Clone)]
struct MockPolicy;

impl<E> Policy<Request, Response, E> for MockPolicy
where
    E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
{
    type Future = FutureResult<Self, ()>;

    fn retry(&self, _req: &Request, _result: Result<&Response, &E>) -> Option<Self::Future> {
        None
    }

    fn clone_request(&self, req: &Request) -> Option<Request> {
        Some(req.clone())
    }
}
