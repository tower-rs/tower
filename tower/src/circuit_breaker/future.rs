use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Instant,
};

use pin_project_lite::pin_project;

use super::service::{CircuitError, CircuitStatus, State};

pin_project! {
    /// Response future for [`CircuitBreaker`].
    ///
    /// [`CircuitBreaker`]: super::service::CircuitBreaker
    pub struct ResponseFuture<F, T, E> {
        #[pin]
        inner: F,
        state: Arc<Mutex<State>>,
        failure_threshold: usize,
        success_threshold: f64,
        _marker: std::marker::PhantomData<fn() -> (T, E)>,
    }
}

impl<F, T, E> ResponseFuture<F, T, E> {
    pub(crate) fn new(
        state: Arc<Mutex<State>>,
        inner: F,
        failure_threshold: usize,
        success_threshold: f64,
    ) -> Self {
        Self {
            inner,
            state,
            failure_threshold,
            success_threshold,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<F, T, E> Future for ResponseFuture<F, T, E>
where
    F: Future<Output = Result<T, E>>,
{
    type Output = Result<T, CircuitError<E>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let failure_threshold = *this.failure_threshold;
        let success_threshold = *this.success_threshold;

        match this.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(resp)) => {
                let mut s = this.state.lock().expect("circuit breaker state poisoned");
                s.push_result(true);
                match s.status {
                    CircuitStatus::HalfOpen if s.success_rate() >= success_threshold => {
                        s.status = CircuitStatus::Closed;
                        s.consecutive_failures = 0;
                        s.last_transition = Instant::now();
                    }
                    CircuitStatus::Closed => {
                        s.consecutive_failures = 0;
                    }
                    _ => {}
                }
                Poll::Ready(Ok(resp))
            }
            Poll::Ready(Err(e)) => {
                let mut s = this.state.lock().expect("circuit breaker state poisoned");
                s.push_result(false);
                s.consecutive_failures += 1;
                s.last_failure = Some(Instant::now());
                match s.status {
                    CircuitStatus::Closed if s.consecutive_failures >= failure_threshold => {
                        s.status = CircuitStatus::Open;
                        s.last_transition = Instant::now();
                    }
                    CircuitStatus::HalfOpen => {
                        s.status = CircuitStatus::Open;
                        s.last_transition = Instant::now();
                    }
                    _ => {}
                }
                Poll::Ready(Err(CircuitError::Inner(e)))
            }
        }
    }
}
