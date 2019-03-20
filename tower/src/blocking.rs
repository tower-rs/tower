use ext::ServiceExt;
use futures::{future, Future};
use tokio::runtime::current_thread::Runtime;
use tower_buffer::Buffer;
use tower_service::Service;

type Error = Box<::std::error::Error + Send + Sync + 'static>;

pub struct BlockingService<S, Request>
where
    S: Service<Request>,
{
    inner: Buffer<S, Request>,
    rt: Runtime,
}

impl<S, Request> BlockingService<S, Request>
where
    S: Service<Request> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: Into<Error> + Send + Sync + 'static,
    Request: Send + 'static,
{
    pub fn new(inner: S) -> Self {
        let mut rt = Runtime::new().unwrap();
        let inner = match rt.block_on(future::lazy(|| Buffer::new(inner, 1))) {
            Ok(s) => s,
            Err(_) => panic!("Unable to spawn"),
        };
        Self { inner, rt }
    }

    pub fn call(&mut self, req: Request) -> Result<S::Response, Error> {
        let svc = self.inner.clone();
        let fut = svc.ready().and_then(move |mut svc| svc.call(req));
        self.rt.block_on(fut)
    }
}
