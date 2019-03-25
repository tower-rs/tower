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
use tower::ServiceExt;
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
    let fut = futures::lazy(|| {
        request().map(|resp| {
            dbg!(resp);
        })
    });
    hyper::rt::run(fut)
}

fn request() -> impl Future<Item = Response<hyper::Body>, Error = ()> {
    let connector = Connector::new(HttpConnector::new(1));
    let hyper = Connect::new(connector, Builder::new());

    // RetryPolicy is a very simple policy that retries `n` times
    // if the response has a 500 status code. Here, `n` is 5.
    let policy = RetryPolicy::new(5);
    // We're calling the tower/examples/server.rs.
    let dst = Destination::try_from_uri(Uri::from_static("http://127.0.0.1:3000")).unwrap();

    // Now, to build the service! We use two BufferLayers in order to:
    // - provide backpressure for the RateLimitLayer, RateLimitLayer, and InFlightLimitLayer
    // - meet `RetryLayer`'s requirement that our service implement `Service + Clone`
    // - ..and to provide cheap clones on the service.
    let maker = ServiceBuilder::new()
        .layer(BufferLayer::new(5))
        .layer(RateLimitLayer::new(5, Duration::from_secs(1)))
        .layer(InFlightLimitLayer::new(5))
        .layer(RetryLayer::new(policy))
        .layer(BufferLayer::new(5))
        .build_maker(hyper);

    // `Reconnect` accepts a destination and a MakeService, creating a new service
    // any time the connection encounters an error.
    let client = Reconnect::new(maker, dst);

    let request = Request::builder()
        .method("GET")
        .body(Body::from(Vec::new()))
        .unwrap();

    // we check to see if the client is ready to accept requests.
    client
        .ready()
        .map_err(|e| panic!("Service is not ready: {:?}", e))
        .and_then(|mut c| c.call(request).map_err(|e| panic!("{:?}", e)))
}
