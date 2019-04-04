use super::Error;
use super::future::ResponseFuture;

use tower_service::Service;

use futures::Poll;
use std::sync::Arc;
use tokio_sync::semaphore::{self, Semaphore};

#[derive(Debug)]
pub struct LimitConcurrency<T> {
    inner: T,
    limit: Limit,
}

#[derive(Debug)]
struct Limit {
    semaphore: Arc<Semaphore>,
    permit: semaphore::Permit,
}

impl<T> LimitConcurrency<T> {
    /// Create a new rate limiter
    pub fn new<Request>(inner: T, max: usize) -> Self
    where
        T: Service<Request>,
    {
        LimitConcurrency {
            inner,
            limit: Limit {
                semaphore: Arc::new(Semaphore::new(max)),
                permit: semaphore::Permit::new(),
            },
        }
    }

    /// Get a reference to the inner service
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Get a mutable reference to the inner service
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Consume `self`, returning the inner service
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<S, Request> Service<Request> for LimitConcurrency<S>
where
    S: Service<Request>,
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        try_ready!(self
            .limit
            .permit
            .poll_acquire(&self.limit.semaphore)
            .map_err(Error::from));

        self.inner.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Make sure a permit has been acquired
        if self
            .limit
            .permit
            .try_acquire(&self.limit.semaphore)
            .is_err()
        {
            panic!("max requests in-flight; poll_ready must be called first");
        }

        // Call the inner service
        let future = self.inner.call(request);

        // Forget the permit, the permit will be returned when
        // `future::ResponseFuture` is dropped.
        self.limit.permit.forget();

        ResponseFuture::new(future, self.limit.semaphore.clone())
    }
}

impl<S> Clone for LimitConcurrency<S>
where
    S: Clone,
{
    fn clone(&self) -> LimitConcurrency<S> {
        LimitConcurrency {
            inner: self.inner.clone(),
            limit: Limit {
                semaphore: self.limit.semaphore.clone(),
                permit: semaphore::Permit::new(),
            },
        }
    }
}

impl Drop for Limit {
    fn drop(&mut self) {
        self.permit.release(&self.semaphore);
    }
}
