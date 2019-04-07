use futures::{future, Future, Poll, Stream};
use hyper::{self, Body, Request, Response};
use tokio_tcp::TcpListener;
use tower::{builder::ServiceBuilder, Service};
use tower_hyper::{body::LiftBody, server::Server};

fn main() {
    hyper::rt::run(future::lazy(|| {
        let addr = "127.0.0.1:3000".parse().unwrap();
        let bind = TcpListener::bind(&addr).expect("bind");

        println!("Listening on http://{}", addr);

        let maker = ServiceBuilder::new()
            .concurrency_limit(5)
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
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _: ()) -> Self::Future {
        Box::new(future::ok(Svc))
    }
}
