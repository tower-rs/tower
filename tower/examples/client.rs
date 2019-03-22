extern crate futures;
extern crate hyper;
extern crate tower;
extern crate tower_buffer;
extern crate tower_hyper;
extern crate tower_in_flight_limit;
extern crate tower_rate_limit;
extern crate tower_reconnect;
extern crate tower_retry;
extern crate tower_service;

use futures::Future;
use hyper::client::connect::Destination;
use hyper::client::HttpConnector;
use hyper::{Request, Response, Uri};
use std::time::Duration;
use tower::builder::ServiceBuilder;
use tower_buffer::BufferLayer;
use tower_hyper::client::{Builder, Connect};
use tower_hyper::retry::{Body, RetryPolicy};
use tower_hyper::util::Connector;
use tower_in_flight_limit::InFlightLimitLayer;
use tower_rate_limit::RateLimitLayer;
use tower_reconnect::Reconnect;
use tower_retry::RetryLayer;
use tower_service::Service;

fn main() {
    hyper::rt::run(futures::lazy(|| request().map(|_| ())))
}

fn request() -> impl Future<Item = Response<hyper::Body>, Error = ()> {
    let connector = Connector::new(HttpConnector::new(1));
    let hyper = Connect::new(connector, Builder::new());

    let policy = RetryPolicy::new(5);
    let dst = Destination::try_from_uri(Uri::from_static("http://127.0.0.1:3000")).unwrap();

    let maker = ServiceBuilder::new()
        .chain(RateLimitLayer::new(5, Duration::from_secs(1)))
        .chain(InFlightLimitLayer::new(5))
        .chain(RetryLayer::new(policy))
        .chain(BufferLayer::new(5))
        .build_maker(hyper);

    let mut client = Reconnect::new(maker, dst);

    let request = Request::builder()
        .method("GET")
        .body(Body::from(Vec::new()))
        .unwrap();

    client.call(request).map_err(|e| panic!("{:?}", e))
}
