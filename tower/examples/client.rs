use futures::Future;
use hyper::{
    client::{connect::Destination, HttpConnector},
    Request, Response, Uri,
};
use std::time::Duration;
use tower::{builder::ServiceBuilder, ServiceExt};
use tower_hyper::{
    client::{Builder, Connect},
    retry::{Body, RetryPolicy},
    util::Connector,
};

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
    // - provide backpressure for the RateLimitLayer, and ConcurrencyLimitLayer
    // - meet `RetryLayer`'s requirement that our service implement `Service + Clone`
    // - ..and to provide cheap clones on the service.
    let client = ServiceBuilder::new()
        .buffer(5)
        .rate_limit(5, Duration::from_secs(1))
        .concurrency_limit(5)
        .retry(policy)
        .buffer(5)
        .make_service(hyper);

    client
        // Make a `Service` to `dst`...
        .oneshot(dst)
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(|svc| {
            let req = Request::builder()
                .method("GET")
                .body(Body::from(Vec::new()))
                .unwrap();
            // Send the request when `svc` is ready...
            svc.oneshot(req)
        })
        .map_err(|e| panic!("ruh roh: {:?}", e))
}
