use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use pin_project_lite::pin_project;

use super::{
    policy::CircuitPolicy,
    service::{CircuitError, CircuitStatus, SharedState},
};

pin_project! {
    /// Response future for [`CircuitBreaker`].
    ///
    /// [`CircuitBreaker`]: super::service::CircuitBreaker
    pub struct ResponseFuture<F, T, E, P> {
        #[pin]
        inner: F,
        shared: Arc<Mutex<SharedState<P>>>,
        _marker: std::marker::PhantomData<fn() -> (T, E)>,
    }
}

impl<F, T, E, P> ResponseFuture<F, T, E, P> {
    pub(crate) fn new(shared: Arc<Mutex<SharedState<P>>>, inner: F) -> Self {
        Self {
            inner,
            shared,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<F, T, E, P> Future for ResponseFuture<F, T, E, P>
where
    F: Future<Output = Result<T, E>>,
    P: CircuitPolicy,
{
    type Output = Result<T, CircuitError<E>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(resp)) => {
                let mut s = this.shared.lock().expect("circuit breaker state poisoned");
                let should_close = s.policy.on_success();
                if should_close && s.status == CircuitStatus::HalfOpen {
                    s.status = CircuitStatus::Closed;
                }
                Poll::Ready(Ok(resp))
            }
            Poll::Ready(Err(e)) => {
                let mut s = this.shared.lock().expect("circuit breaker state poisoned");
                let should_open = s.policy.on_failure();
                match s.status {
                    // Any failure during a probe reopens immediately —
                    // the backend is not yet ready regardless of threshold.
                    CircuitStatus::HalfOpen => {
                        s.status = CircuitStatus::Open;
                    }
                    CircuitStatus::Closed if should_open => {
                        s.status = CircuitStatus::Open;
                    }
                    _ => {}
                }
                Poll::Ready(Err(CircuitError::Inner(e)))
            }
        }
    }
}
