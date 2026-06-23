use std::{
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use tower_service::Service;

use super::{future::ResponseFuture, policy::CircuitPolicy};

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

/// Shared mutable state between a [`CircuitBreaker`] and its [`ResponseFuture`].
pub(crate) struct SharedState<P> {
    pub(crate) status: CircuitStatus,
    pub(crate) policy: P,
}

/// Tower [`Service`] implementing the circuit-breaker pattern.
///
/// The open/probe/close criteria are driven by a [`CircuitPolicy`], making
/// the triggering logic independently customisable.  The built-in policy is
/// [`ConsecutiveFailures`]; supply any type implementing [`CircuitPolicy`]
/// via [`CircuitBreaker::new`] or [`CircuitBreakerLayer::with_policy`] for
/// custom strategies.
///
/// # Thread safety
///
/// `CircuitBreaker<S, P>` is [`Send`] when both `S` and `P` are [`Send`].
/// This is enforced structurally: the policy is held behind
/// `Arc<Mutex<P>>`, so `Arc<Mutex<P>>: Send` requires `P: Send`.
/// No explicit bound is placed on `P` in the [`Service`] impl, so
/// `!Send` policies can still be used in single-threaded contexts without
/// a compile error.  `P: Sync` is never required.
///
/// See [`CircuitPolicy`] for more detail.
///
/// See the [module documentation](super) for a full example.
///
/// [`ConsecutiveFailures`]: super::ConsecutiveFailures
/// [`CircuitBreakerLayer::with_policy`]: super::CircuitBreakerLayer::with_policy
/// [`CircuitPolicy`]: super::CircuitPolicy
#[derive(Clone)]
pub struct CircuitBreaker<S, P> {
    inner: S,
    pub(crate) shared: Arc<Mutex<SharedState<P>>>,
}

impl<S, P: CircuitPolicy> CircuitBreaker<S, P> {
    /// Wrap `inner` with the given [`CircuitPolicy`].
    pub fn new(inner: S, policy: P) -> Self {
        Self {
            inner,
            shared: Arc::new(Mutex::new(SharedState {
                status: CircuitStatus::Closed,
                policy,
            })),
        }
    }

    /// Return the current [`CircuitStatus`].
    pub fn status(&self) -> CircuitStatus {
        self.shared
            .lock()
            .expect("circuit breaker state poisoned")
            .status
            .clone()
    }

    /// Manually close the circuit (e.g. after operator confirmation that the
    /// backend is healthy).
    ///
    /// Calls [`CircuitPolicy::on_half_open`] to reset any per-window counters
    /// in the policy, then sets the status to [`Closed`][CircuitStatus::Closed].
    pub fn reset(&self) {
        let mut s = self.shared.lock().expect("circuit breaker state poisoned");
        s.policy.on_half_open(); // reuse the window-clear hook
        s.status = CircuitStatus::Closed;
    }
}

impl<S, P, Request> Service<Request> for CircuitBreaker<S, P>
where
    S: Service<Request>,
    P: CircuitPolicy,
{
    type Response = S::Response;
    type Error = CircuitError<S::Error>;
    type Future = ResponseFuture<S::Future, S::Response, S::Error, P>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        {
            let mut s = self.shared.lock().expect("circuit breaker state poisoned");
            if s.status == CircuitStatus::Open {
                if s.policy.should_probe() {
                    s.policy.on_half_open();
                    s.status = CircuitStatus::HalfOpen;
                    // fall through to delegate to inner service
                } else {
                    return Poll::Ready(Err(CircuitError::Open));
                }
            }
        }

        self.inner.poll_ready(cx).map_err(CircuitError::Inner)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        ResponseFuture::new(self.shared.clone(), self.inner.call(req))
    }
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_breaker::{CircuitBreakerLayer, ConsecutiveFailures};
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
        let policy = ConsecutiveFailures::new(2, 0.8, Duration::from_secs(60));
        let cb = CircuitBreaker::new(inner, policy);

        let _ = tower::ServiceExt::oneshot(cb.clone(), "req").await;
        let _ = tower::ServiceExt::oneshot(cb.clone(), "req").await;
        assert_eq!(cb.status(), CircuitStatus::Open);

        cb.reset();
        assert_eq!(cb.status(), CircuitStatus::Closed);
    }

    #[tokio::test]
    async fn custom_policy_is_accepted() {
        // Verify the Service impl compiles and runs with a hand-rolled policy.
        #[derive(Clone)]
        struct AlwaysOpen;
        impl CircuitPolicy for AlwaysOpen {
            fn on_success(&mut self) -> bool { false }
            fn on_failure(&mut self) -> bool { true }
            fn should_probe(&self) -> bool { false }
            fn on_half_open(&mut self) {}
        }

        let inner = tower::service_fn(|_: &'static str| async move { Err::<&str, _>("x") });
        let cb = CircuitBreaker::new(inner, AlwaysOpen);

        // One failure should open the circuit.
        let _ = tower::ServiceExt::oneshot(cb.clone(), "req").await;
        assert_eq!(cb.status(), CircuitStatus::Open);
    }
}
