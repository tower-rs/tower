//! State machine instrumentation.

/// Consumes the state machine events. May used for metrics and/or logs.
pub trait Instrument {
    /// Calls when state machine reject a call.
    fn on_call_rejected(&self);

    /// Calls when the circuit breaker become to open state.
    fn on_open(&self);

    /// Calls when the circuit breaker become to half open state.
    fn on_half_open(&self);

    /// Calls when the circuit breaker become to closed state.
    fn on_closed(&self);
}

/// An instrumentation which does noting.
impl Instrument for () {
    #[inline]
    fn on_call_rejected(&self) {}

    #[inline]
    fn on_open(&self) {}

    #[inline]
    fn on_half_open(&self) {}

    #[inline]
    fn on_closed(&self) {}
}
