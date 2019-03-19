use tokio::runtime::current_thread::Runtime;
use tower_service::Service;
use tower_service_util::BoxService;

pub struct BlockingSvc<Request, Response, Err> {
    inner: BoxService<Request, Response, Err>,
    rt: Runtime,
}

impl<Request, Response, Err> BlockingSvc<Request, Response, Err> {
    pub fn new<S>(inner: S) -> Self
    where
        S: Service<Request, Response = Response, Error = Err> + Send + 'static,
        S::Future: Send + 'static,
    {
        Self {
            inner: BoxService::new(inner),
            rt: Runtime::new().unwrap(),
        }
    }

    pub fn call(&mut self, req: Request) -> Result<Response, Err> {
        self.inner.poll_ready()?;
        self.rt.block_on(self.inner.call(req))
    }
}
