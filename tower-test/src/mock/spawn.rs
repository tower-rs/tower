use std::task::Poll;
use tokio_test::task;
use tower_service::Service;

#[derive(Debug)]
pub struct Spawn<T> {
    inner: T,
    task: task::Spawn<()>,
}

impl<T> Spawn<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            task: task::spawn(()),
        }
    }

    pub fn is_woken(&self) -> bool {
        self.task.is_woken()
    }

    pub fn waker_ref_count(&self) -> usize {
        self.task.waker_ref_count()
    }

    pub fn poll_ready<Request>(&mut self) -> Poll<Result<(), T::Error>>
    where
        T: Service<Request>,
    {
        let task = &mut self.task;
        let inner = &mut self.inner;

        task.enter(|cx, _| inner.poll_ready(cx))
    }

    pub fn call<Request>(&mut self, req: Request) -> T::Future
    where
        T: Service<Request>,
    {
        self.inner.call(req)
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}
