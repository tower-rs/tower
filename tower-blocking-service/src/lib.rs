use lazy_static::lazy_static;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use tower_service::Service;
use tower_service_util::BoxService;

lazy_static! {
    static ref RUNTIME: Arc<Mutex<Runtime>> = Arc::new(Mutex::new(
        Runtime::new().expect("Runtime could not be created")
    ));
}

trait BlockingService<Request> {
    type Response;
    type Error;

    fn call(&mut self, req: Request) -> Result<Self::Response, Self::Error>;
}

struct BlockingSvc<T, U, E> {
    inner: BoxService<T, U, E>,
}

impl<T, U, E> BlockingSvc<T, U, E> {
    fn new<S>(inner: S) -> Self
    where
        S: Service<T, Response = U, Error = E> + Send + 'static,
        S::Future: Send + 'static,
    {
        Self {
            inner: BoxService::new(inner),
        }
    }
}

impl<Request, U, E> BlockingService<Request> for BlockingSvc<Request, U, E>
where
    E: Send + 'static,
    U: Send + 'static,
{
    type Response = U;
    type Error = E;

    fn call(&mut self, req: Request) -> Result<Self::Response, Self::Error> {
        RUNTIME.lock().unwrap().block_on(self.inner.call(req))
    }
}
