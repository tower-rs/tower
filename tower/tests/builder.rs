extern crate futures;
extern crate tokio;
extern crate tower;
extern crate tower_buffer;
extern crate tower_in_flight_limit;
extern crate tower_layer;
extern crate tower_rate_limit;
extern crate tower_reconnect;
extern crate tower_service;

use futures::future::{self, FutureResult};
use futures::prelude::*;
use std::time::Duration;
use tower::builder::ServiceBuilder;
use tower_buffer::BufferLayer;
use tower_in_flight_limit::InFlightLimitLayer;
use tower_layer::util::Never;
use tower_rate_limit::RateLimitLayer;
use tower_reconnect::Reconnect;
use tower_service::*;

#[derive(Debug)]
struct MockMaker;
impl Service<()> for MockMaker {
    type Response = MockSvc;
    type Error = Never;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _: ()) -> Self::Future {
        future::ok(MockSvc)
    }
}

#[derive(Debug)]
struct Request;
#[derive(Debug)]
struct Response;
#[derive(Debug)]
struct MockSvc;
impl Service<Request> for MockSvc {
    type Response = Response;
    type Error = Never;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _: Request) -> Self::Future {
        future::ok(Response)
    }
}

#[test]
fn builder_basic() {
    tokio::run(future::lazy(|| {
        let maker = ServiceBuilder::new(MockMaker)
            .add(BufferLayer::new(5))
            .add(InFlightLimitLayer::new(5))
            .add(RateLimitLayer::new(5, Duration::from_secs(1)))
            .build();

        let mut client = Reconnect::new(maker, ());

        client.poll_ready().unwrap();
        client
            .call(Request)
            .map(|_| ())
            .map_err(|_| panic!("this is bad"))
    }));
}
