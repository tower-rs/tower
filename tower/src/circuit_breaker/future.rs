use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use pin_project_lite::pin_project;
use tokio::sync::RwLock;

use super::service::{CircuitError, CircuitStatus, State};

pin_project! {
    /// Response future for [`CircuitBreaker`].
    ///
    /// [`CircuitBreaker`]: super::service::CircuitBreaker
    pub struct ResponseFuture<F, T, E> {
        #[pin]
        inner: F,
        state: Arc<RwLock<State>>,
        failure_threshold: usize,
        success_threshold: f64,
        timeout: Duration,
        /// Set to true once we've checked the circuit state and decided to proceed.
        gate_checked: bool,
        _marker: std::marker::PhantomData<fn() -> (T, E)>,
    }
}

impl<F, T, E> ResponseFuture<F, T, E> {
    pub(crate) fn new(
        state: Arc<RwLock<State>>,
        inner: F,
        failure_threshold: usize,
        success_threshold: f64,
        timeout: Duration,
    ) -> Self {
        Self {
            inner,
            state,
            failure_threshold,
            success_threshold,
            timeout,
            gate_checked: false,
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

        if !*this.gate_checked {
            // Non-blocking read-lock check.
            match this.state.try_read() {
                Err(_) => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Ok(guard) => {
                    match guard.status {
                        CircuitStatus::Open => {
                            let elapsed = guard
                                .last_failure
                                .map(|t| t.elapsed())
                                .unwrap_or(Duration::ZERO);

                            if elapsed < *this.timeout {
                                return Poll::Ready(Err(CircuitError::Open));
                            }

                            // Timeout elapsed — transition to HalfOpen asynchronously.
                            drop(guard);
                            let arc = this.state.clone();
                            tokio::spawn(async move {
                                let mut s = arc.write().await;
                                if s.status == CircuitStatus::Open {
                                    s.status = CircuitStatus::HalfOpen;
                                    s.window.clear();
                                    s.consecutive_failures = 0;
                                    s.last_transition = Instant::now();
                                }
                            });
                        }
                        CircuitStatus::Closed | CircuitStatus::HalfOpen => {
                            drop(guard);
                        }
                    }
                    *this.gate_checked = true;
                }
            }
        }

        let failure_threshold = *this.failure_threshold;
        let success_threshold = *this.success_threshold;

        match this.inner.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(resp)) => {
                let arc = this.state.clone();
                tokio::spawn(async move {
                    let mut s = arc.write().await;
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
                });
                Poll::Ready(Ok(resp))
            }
            Poll::Ready(Err(e)) => {
                let arc = this.state.clone();
                tokio::spawn(async move {
                    let mut s = arc.write().await;
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
                });
                Poll::Ready(Err(CircuitError::Inner(e)))
            }
        }
    }
}
