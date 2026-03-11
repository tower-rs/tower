use std::time::{Duration, Instant};

/// Determines when a [`CircuitBreaker`] should open, probe, and close.
///
/// Implement this trait to create custom circuit-breaking strategies —
/// for example latency-based triggers, error-rate thresholds, or a
/// manual operator-driven switch.  The built-in [`ConsecutiveFailures`]
/// policy is a good starting point for most use cases.
///
/// # Thread safety
///
/// `CircuitPolicy` does **not** require [`Send`] or [`Sync`] as supertraits,
/// so single-threaded or `!Send` implementations are valid.  However, because
/// the policy is stored inside `Arc<Mutex<…>>` within [`CircuitBreaker`],
/// the compiler will automatically require `P: Send` whenever
/// `CircuitBreaker<S, P>` is sent across threads (e.g. handed to
/// `tokio::spawn` or used with a multi-threaded runtime).  `P: Sync` is
/// **not** needed — `Mutex` provides the necessary exclusion.
///
/// In practice, any policy that holds only owned data will be `Send`
/// automatically.  If you store a raw pointer or `Rc` in your policy, it will
/// not be usable in a multi-threaded context — the compiler will tell you so
/// at the call site.
///
/// # Relationship to [`tower::retry::budget`]
///
/// [`Budget`][budget] governs *retry worthiness*: it caps the ratio of
/// retried requests to original requests, preventing retry amplification.
/// A circuit breaker governs *traffic admission*: it gates **all**
/// requests (including first attempts) when a backend is known to be
/// unhealthy.
///
/// The two compose naturally:
///
/// - A budget limits how aggressively clients retry individual requests.
/// - A circuit breaker stops all traffic once failure is systemic, giving
///   the backend time to recover without being drowned in retried load.
///
/// Using a circuit breaker *without* a budget still exposes you to retry
/// amplification from layers above; the combination of both provides full
/// protection against retry storms.
///
/// ```text
/// ┌──────────────────────────────────────┐
/// │           ServiceBuilder             │
/// │  .layer(CircuitBreakerLayer::…)  ◄── gates all traffic when open
/// │  .layer(RetryLayer::new(policy)) ◄── budget inside policy caps retries
/// │  .service_fn(my_backend)             │
/// └──────────────────────────────────────┘
/// ```
///
/// [budget]: crate::retry::budget
/// [`CircuitBreaker`]: super::service::CircuitBreaker
pub trait CircuitPolicy {
    /// Called after a **successful** response from the inner service.
    ///
    /// Return `true` to signal that the circuit should close.  This is
    /// acted upon only while the circuit is
    /// [`HalfOpen`][crate::circuit_breaker::CircuitStatus::HalfOpen];
    /// returning `true` from a [`Closed`][crate::circuit_breaker::CircuitStatus::Closed]
    /// state is a no-op.
    fn on_success(&mut self) -> bool;

    /// Called after a **failed** response from the inner service.
    ///
    /// Return `true` to signal that the circuit should open.  This is
    /// acted upon when the circuit is
    /// [`Closed`][crate::circuit_breaker::CircuitStatus::Closed].
    ///
    /// Any failure while the circuit is
    /// [`HalfOpen`][crate::circuit_breaker::CircuitStatus::HalfOpen]
    /// always reopens it, regardless of the return value — the probe
    /// failed, so the backend is not yet ready.
    fn on_failure(&mut self) -> bool;

    /// Called while the circuit is [`Open`][crate::circuit_breaker::CircuitStatus::Open].
    ///
    /// Return `true` to allow a probe request through (transitions the
    /// circuit to [`HalfOpen`][crate::circuit_breaker::CircuitStatus::HalfOpen]).
    fn should_probe(&self) -> bool;

    /// Called immediately after the circuit transitions to
    /// [`HalfOpen`][crate::circuit_breaker::CircuitStatus::HalfOpen].
    ///
    /// Use this hook to reset per-window counters so that the recovery
    /// success rate is measured only from post-recovery probes, not from
    /// stale pre-outage history.
    fn on_half_open(&mut self);
}

// ---------------------------------------------------------------------------
// ConsecutiveFailures — the built-in policy
// ---------------------------------------------------------------------------

/// A [`CircuitPolicy`] that opens the circuit after *N* consecutive failures
/// and closes it again once a sufficient fraction of probes succeed.
///
/// # Parameters
///
/// | Parameter | Description |
/// |---|---|
/// | `failure_threshold` | Number of consecutive failures needed to open the circuit. |
/// | `success_threshold` | Fraction of HalfOpen probes (0.0–1.0) that must succeed to close. |
/// | `timeout` | How long to stay Open before sending the first probe. |
///
/// # Example
///
/// ```rust,ignore
/// use tower::circuit_breaker::{CircuitBreakerLayer, ConsecutiveFailures};
/// use std::time::Duration;
///
/// let policy = ConsecutiveFailures::new(5, 0.8, Duration::from_secs(30));
/// let layer  = CircuitBreakerLayer::with_policy(policy);
/// ```
#[derive(Clone, Debug)]
pub struct ConsecutiveFailures {
    failure_threshold: usize,
    success_threshold: f64,
    timeout: Duration,
    consecutive_failures: usize,
    /// Set when the circuit opens; used by `should_probe`.
    open_since: Option<Instant>,
    /// Sliding window of outcomes during HalfOpen (max 100 entries).
    window: Vec<bool>,
}

impl ConsecutiveFailures {
    /// Create a new [`ConsecutiveFailures`] policy.
    pub fn new(failure_threshold: usize, success_threshold: f64, timeout: Duration) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            timeout,
            consecutive_failures: 0,
            open_since: None,
            window: Vec::with_capacity(32),
        }
    }
}

impl CircuitPolicy for ConsecutiveFailures {
    fn on_success(&mut self) -> bool {
        self.consecutive_failures = 0;
        self.window.push(true);
        if self.window.len() > 100 {
            self.window.remove(0);
        }
        let rate = self.window.iter().filter(|&&v| v).count() as f64
            / self.window.len() as f64;
        rate >= self.success_threshold
    }

    fn on_failure(&mut self) -> bool {
        self.consecutive_failures += 1;
        self.window.push(false);
        if self.window.len() > 100 {
            self.window.remove(0);
        }
        let should_open = self.consecutive_failures >= self.failure_threshold;
        if should_open {
            self.open_since = Some(Instant::now());
        }
        should_open
    }

    fn should_probe(&self) -> bool {
        self.open_since
            .map(|t| t.elapsed() >= self.timeout)
            .unwrap_or(false)
    }

    fn on_half_open(&mut self) {
        self.window.clear();
        self.consecutive_failures = 0;
    }
}
