use std::{
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use tower_service::Service;

use super::future::ResponseFuture;

/// Current state of a [`CircuitBreaker`] service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitStatus {
    /// Normal operation — requests flow through.
    Closed,
    /// Service is unhealthy — requests are rejected immediately.
    Open,
    /// One probe request is allowed through to test recovery.
    HalfOpen,
}

/// Error type returned by a [`CircuitBreaker`] service.
#[derive(Debug)]
pub enum CircuitError<E> {
    /// The circuit is open; the inner service was not called.
    Open,
    /// The inner service returned this error.
    Inner(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "circuit breaker is open"),
            Self::Inner(e) => write!(f, "{e}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CircuitError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Inner(e) => Some(e),
            Self::Open => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct State {
    pub(crate) status: CircuitStatus,
    pub(crate) consecutive_failures: usize,
    pub(crate) last_failure: Option<Instant>,
    pub(crate) last_transition: Instant,
    /// Sliding window: `true` = success, `false` = failure (max 100 entries).
    pub(crate) window: Vec<bool>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            status: CircuitStatus::Closed,
            consecutive_failures: 0,
            last_failure: None,
            last_transition: Instant::now(),
            window: Vec::with_capacity(100),
        }
    }

    pub(crate) fn push_result(&mut self, success: bool) {
        self.window.push(success);
        if self.window.len() > 100 {
            self.window.remove(0);
        }
    }

    pub(crate) fn success_rate(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        self.window.iter().filter(|&&v| v).count() as f64 / self.window.len() as f64
    }
}

/// Tower [`Service`] that implements the circuit-breaker pattern.
///
/// See the [module documentation](super) for a full example.
#[derive(Clone)]
#[cfg_attr(
    any(test, feature = "circuit-breaker"),
    allow(missing_debug_implementations)
)]
pub struct CircuitBreaker<S> {
    inner: S,
    pub(crate) state: Arc<Mutex<State>>,
    pub(crate) failure_threshold: usize,
    pub(crate) success_threshold: f64,
    pub(crate) timeout: Duration,
}

impl<S> CircuitBreaker<S> {
    /// Wrap `inner` in a circuit breaker.
    pub fn new(
        inner: S,
        failure_threshold: usize,
        success_threshold: f64,
        timeout: Duration,
    ) -> Self {
        Self {
            inner,
            state: Arc::new(Mutex::new(State::new())),
            failure_threshold,
            success_threshold,
            timeout,
        }
    }

    /// Return the current [`CircuitStatus`].
    pub fn status(&self) -> CircuitStatus {
        self.state
            .lock()
            .expect("circuit breaker state poisoned")
            .status
            .clone()
    }

    /// Manually close the circuit (e.g. after operator confirmation).
    pub fn reset(&self) {
        let mut s = self.state.lock().expect("circuit breaker state poisoned");
        s.status = CircuitStatus::Closed;
        s.consecutive_failures = 0;
        s.window.clear();
        s.last_transition = Instant::now();
    }
}

impl<S, Request> Service<Request> for CircuitBreaker<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = CircuitError<S::Error>;
    type Future = ResponseFuture<S::Future, S::Response, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Check circuit state synchronously before delegating to inner.
        {
            let mut s = self.state.lock().expect("circuit breaker state poisoned");
            match s.status {
                CircuitStatus::Open => {
                    let elapsed = s
                        .last_failure
                        .map(|t| t.elapsed())
                        .unwrap_or(Duration::ZERO);
                    if elapsed < self.timeout {
                        return Poll::Ready(Err(CircuitError::Open));
                    }
                    // Timeout elapsed — transition to HalfOpen.
                    s.status = CircuitStatus::HalfOpen;
                    s.window.clear();
                    s.consecutive_failures = 0;
                    s.last_transition = Instant::now();
                }
                CircuitStatus::Closed | CircuitStatus::HalfOpen => {}
            }
        }

        self.inner.poll_ready(cx).map_err(CircuitError::Inner)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        ResponseFuture::new(
            self.state.clone(),
            self.inner.call(req),
            self.failure_threshold,
            self.success_threshold,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_breaker::CircuitBreakerLayer;
    use std::time::Duration;
    use tower::{ServiceBuilder, ServiceExt};

    #[tokio::test]
    async fn closed_passes_requests_through() {
        let mut svc = ServiceBuilder::new()
            .layer(CircuitBreakerLayer::new(5, 0.8, Duration::from_secs(60)))
            .service_fn(|req: &'static str| async move { Ok::<_, &'static str>(req) });

        let resp = svc.ready().await.unwrap().call("hello").await;
        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn opens_after_failure_threshold() {
        let mut svc = ServiceBuilder::new()
            .layer(CircuitBreakerLayer::new(3, 0.8, Duration::from_secs(60)))
            .service_fn(|_: &'static str| async move { Err::<&str, _>("fail") });

        for _ in 0..3 {
            let _ = svc.ready().await.unwrap().call("req").await;
        }

        // Circuit is now Open — poll_ready should reject.
        let result = svc.ready().await;
        assert!(matches!(result, Err(CircuitError::Open)));
    }

    #[tokio::test]
    async fn manual_reset_closes_circuit() {
        let inner = tower::service_fn(|_: &'static str| async move { Err::<&str, _>("fail") });
        let cb = CircuitBreaker::new(inner, 2, 0.8, Duration::from_secs(60));

        // Open the circuit.
        let _ = tower::ServiceExt::oneshot(cb.clone(), "req").await;
        let _ = tower::ServiceExt::oneshot(cb.clone(), "req").await;
        assert_eq!(cb.status(), CircuitStatus::Open);

        cb.reset();
        assert_eq!(cb.status(), CircuitStatus::Closed);
    }
}
