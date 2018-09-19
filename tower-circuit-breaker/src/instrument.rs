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

pub use self::observe::Observer;

mod observe {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::Instrument;

    const CLOSED: usize = 0;
    const OPEN: usize = 1;
    const HALF_OPEN: usize = 2;

    #[derive(Debug)]
    struct Shared {
        state: AtomicUsize,
        rejected_calls: AtomicUsize,
    }

    /// An instrumentation which record the state machine state and rejected calls.
    #[derive(Clone, Debug)]
    pub struct Observer {
        shared: Arc<Shared>,
    }

    impl Default for Observer {
        fn default() -> Self {
            Self {
                shared: Arc::new(Shared {
                    state: AtomicUsize::new(CLOSED),
                    rejected_calls: AtomicUsize::new(0),
                }),
            }
        }
    }

    impl Observer {
        /// Checks that the current state is closed.
        pub fn is_closed(&self) -> bool {
            match self.shared.state.load(Ordering::Acquire) {
                CLOSED => true,
                _ => false,
            }
        }

        /// Checks that the current state is open.
        pub fn is_open(&self) -> bool {
            match self.shared.state.load(Ordering::Acquire) {
                OPEN => true,
                _ => false,
            }
        }

        /// Checks that the current state is half open.
        pub fn is_half_open(&self) -> bool {
            match self.shared.state.load(Ordering::Acquire) {
                HALF_OPEN => true,
                _ => false,
            }
        }

        /// Returns a number of rejected calls.
        pub fn rejected_calls(&self) -> usize {
            self.shared.rejected_calls.load(Ordering::Acquire)
        }
    }

    impl Instrument for Observer {
        fn on_call_rejected(&self) {
            self.shared.rejected_calls.fetch_add(1, Ordering::SeqCst);
        }

        fn on_open(&self) {
            self.shared.state.store(OPEN, Ordering::SeqCst);
        }

        fn on_half_open(&self) {
            self.shared.state.store(HALF_OPEN, Ordering::SeqCst);
        }

        fn on_closed(&self) {
            self.shared.state.store(CLOSED, Ordering::SeqCst);
        }
    }
}
