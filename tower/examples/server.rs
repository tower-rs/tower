extern crate futures;
extern crate hyper;
extern crate tokio_tcp;
extern crate tower;
extern crate tower_hyper;
extern crate tower_limit;
extern crate tower_service;

use futures::{future, Future, Poll, Stream};
use hyper::{Body, Request, Response};
use tokio_tcp::TcpListener;
use tower::builder::ServiceBuilder;
use tower_hyper::body::LiftBody;
use tower_hyper::server::Server;
use tower_limit::concurrency::ConcurrencyLimitLayer;
use tower_service::Service;

fn main() {
    hyper::rt::run(future::lazy(|| {
        let addr = "127.0.0.1:3000".parse().unwrap();
        let bind = TcpListener::bind(&addr).expect("bind");

        println!("Listening on http://{}", addr);

        let maker = ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(5))
            .make_service(MakeSvc);

        let server = Server::new(maker);

        bind.incoming()
            .fold(server, |mut server, stream| {
                if let Err(e) = stream.set_nodelay(true) {
                    return Err(e);
                }

                hyper::rt::spawn(
                    server
                        .serve(stream)
                        .map_err(|e| panic!("Server error {:?}", e)),
                );

                Ok(server)
            })
            .map_err(|e| panic!("serve error: {:?}", e))
            .map(|_| {})
    }));
}

struct Svc;
impl Service<Request<Body>> for Svc {
    type Response = Response<LiftBody<Body>>;
    type Error = hyper::Error;
    type Future = future::FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _req: Request<Body>) -> Self::Future {
        let res = Response::new(LiftBody::new(Body::from("Hello World!")));
        future::ok(res)
    }
}

struct MakeSvc;
impl Service<()> for MakeSvc {
    type Response = Svc;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _: ()) -> Self::Future {
        Box::new(future::ok(Svc))
    }
}
