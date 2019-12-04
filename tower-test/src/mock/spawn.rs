//! Spawn mock services onto a mock task.

use std::task::Poll;
use tokio_test::task;
use tower_service::Service;

/// Service spawned on a mock task
#[derive(Debug)]
pub struct Spawn<T> {
    inner: T,
    task: task::Spawn<()>,
}

impl<T> Spawn<T> {
    /// Create a new spawn.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            task: task::spawn(()),
        }
    }

    /// Check if this service has been woken up.
    pub fn is_woken(&self) -> bool {
        self.task.is_woken()
    }

    /// Get how many futurs are holding onto the waker.
    pub fn waker_ref_count(&self) -> usize {
        self.task.waker_ref_count()
    }

    /// Poll this service ready.
    pub fn poll_ready<Request>(&mut self) -> Poll<Result<(), T::Error>>
    where
        T: Service<Request>,
    {
        let task = &mut self.task;
        let inner = &mut self.inner;

        task.enter(|cx, _| inner.poll_ready(cx))
    }

    /// Call the inner Service.
    pub fn call<Request>(&mut self, req: Request) -> T::Future
    where
        T: Service<Request>,
    {
        self.inner.call(req)
    }

    /// Get the inner service.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Get a reference to the inner service.
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Get a mutable reference to the inner service.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: Clone> Clone for Spawn<T> {
    fn clone(&self) -> Self {
        Spawn::new(self.inner.clone())
    }
}
