use futures::{future, Future};
use tokio::runtime::current_thread::Runtime;
use tower_buffer::Buffer;
use tower_service::Service;
use util::ServiceExt;

type Error = Box<::std::error::Error + Send + Sync + 'static>;

/// `BlockingService` converts a Tower service—which is, by default, asynchronous—into a
/// blocking, *synchronous* service. `BlockingService` should be used as a top-level layer.
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
    pub fn new(inner: S, max_concurrent_requests: usize) -> Self {
        let mut rt = Runtime::new().expect("Unable to create runtime");
        let inner = match rt.block_on(future::lazy(|| Buffer::new(inner, max_concurrent_requests)))
        {
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
