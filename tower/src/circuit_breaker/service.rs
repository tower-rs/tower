use std::{
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use tokio::sync::RwLock;
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
pub struct CircuitBreaker<S> {
    inner: S,
    pub(crate) state: Arc<RwLock<State>>,
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
            state: Arc::new(RwLock::new(State::new())),
            failure_threshold,
            success_threshold,
            timeout,
        }
    }

    /// Return the current [`CircuitStatus`].
    pub async fn status(&self) -> CircuitStatus {
        self.state.read().await.status.clone()
    }

    /// Manually close the circuit (e.g. after operator confirmation).
    pub async fn reset(&self) {
        let mut s = self.state.write().await;
        s.status = CircuitStatus::Closed;
        s.consecutive_failures = 0;
        s.window.clear();
        s.last_transition = Instant::now();
    }
}

impl<S, Request> Service<Request> for CircuitBreaker<S>
where
    S: Service<Request> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    S::Response: Send + 'static,
    Request: Send + 'static,
{
    type Response = S::Response;
    type Error = CircuitError<S::Error>;
    type Future = ResponseFuture<S::Future, S::Response, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(CircuitError::Inner)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let state = self.state.clone();
        let failure_threshold = self.failure_threshold;
        let success_threshold = self.success_threshold;
        let timeout = self.timeout;

        let mut inner = self.inner.clone();
        std::mem::swap(&mut inner, &mut self.inner);

        ResponseFuture::new(state, inner.call(req), failure_threshold, success_threshold, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tower::{ServiceBuilder, ServiceExt, service_fn};

    #[tokio::test]
    async fn closed_passes_requests_through() {
        let mut svc = ServiceBuilder::new()
            .layer(super::super::layer::CircuitBreakerLayer::new(5, 0.8, Duration::from_secs(60)))
            .service_fn(|req: &'static str| async move { Ok::<_, &'static str>(req) });

        let resp = svc.ready().await.unwrap().call("hello").await;
        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn opens_after_failure_threshold() {
        let mut svc = ServiceBuilder::new()
            .layer(super::super::layer::CircuitBreakerLayer::new(3, 0.8, Duration::from_secs(60)))
            .service_fn(|_: &'static str| async move { Err::<&str, _>("fail") });

        for _ in 0..3 {
            let _ = svc.ready().await.unwrap().call("req").await;
        }

        let result = svc.ready().await.unwrap().call("req").await;
        assert!(matches!(result, Err(CircuitError::Open)));
    }
}
