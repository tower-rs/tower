extern crate tokio_timer;
extern crate tokio_executor;
extern crate tower_circuit_breaker;

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tower_circuit_breaker::failure_accrual::consecutive_failures;
use tower_circuit_breaker::{CircuitBreaker, State, Instrumentation, backoff};

/// Perform `Closed` -> `Open` -> `HalfOpen` -> `Open` -> `HalfOpen` -> `Closed` transitions.
#[test]
fn state_machine() {
    let observe = Observer::new();
    let backoff = backoff::exponential(5.seconds(), 300.seconds());
    let policy = consecutive_failures(3, backoff);

    let mut circuit_breaker = CircuitBreaker::new(policy)
        .with_instrumentation(observe.clone());

    mock_clock::freeze(move |time| {
        assert_eq!(true, circuit_breaker.is_call_permitted());

        // Perform success requests. the circuit breaker must be closed.
        for _i in 0..10 {
            assert_eq!(true, circuit_breaker.is_call_permitted());
            circuit_breaker.on_success();
            assert_eq!(true, observe.is_closed());
        }

        // Perform failed requests, the circuit breaker still closed.
        for _i in 0..2 {
            assert_eq!(true, circuit_breaker.is_call_permitted());
            circuit_breaker.on_error();
            assert_eq!(true, observe.is_closed());
        }

        // Perform a failed request and transit to the open state for 5s.
        assert_eq!(true, circuit_breaker.is_call_permitted());
        circuit_breaker.on_error();
        assert_eq!(true, observe.is_open());

        // Reject call attempts, the circuit breaker in open state.
        for i in 0..10 {
            assert_eq!(false, circuit_breaker.is_call_permitted());
            assert_eq!(i + 1, observe.rejected_calls());
        }

        // Wait 2s, the circuit breaker still open.
        time.advance(2.seconds());
        assert_eq!(false, circuit_breaker.is_call_permitted());
        assert_eq!(true, observe.is_open());

        // Wait 4s (6s total), the circuit breaker now in the half open state.
        time.advance(4.seconds());
        assert_eq!(true, circuit_breaker.is_call_permitted());
        assert_eq!(true, observe.is_half_open());

        // Perform a failed request and transit back to the open state for 10s.
        circuit_breaker.on_error();
        assert_eq!(false, circuit_breaker.is_call_permitted());
        assert_eq!(true, observe.is_open());

        // Wait 5s, the circuit breaker still open.
        time.advance(5.seconds());
        assert_eq!(false, circuit_breaker.is_call_permitted());
        assert_eq!(true, observe.is_open());

        // Wait 6s (11s total), the circuit breaker now in the half open state.
        time.advance(6.seconds());
        assert_eq!(true, circuit_breaker.is_call_permitted());
        assert_eq!(true, observe.is_half_open());

        // Perform a success request and transit to the closed state.
        circuit_breaker.on_success();
        assert_eq!(true, circuit_breaker.is_call_permitted());
        assert_eq!(true, observe.is_closed());

        // Perform success requests.
        for _i in 0..10 {
            assert_eq!(true, circuit_breaker.is_call_permitted());
            circuit_breaker.on_success();
        }
    });
}

#[derive(Clone, Debug)]
struct Observer {
    state: Arc<Mutex<State>>,
    rejected_calls: Arc<AtomicUsize>
}

impl Observer {
    fn new() -> Self {
        Observer {
            state: Arc::new(Mutex::new(State::Closed)),
            rejected_calls: Arc::new(AtomicUsize::new(0))
        }
    }

    fn is_closed(&self) -> bool {
        self.state.lock().unwrap().is_closed()
    }

    fn is_open(&self) -> bool {
        self.state.lock().unwrap().is_open()
    }

    fn is_half_open(&self) -> bool {
        self.state.lock().unwrap().is_half_open()
    }

    fn rejected_calls(&self) -> usize {
        self.rejected_calls.load(Ordering::SeqCst)
    }
}

impl Instrumentation for Observer {
    fn on_transition(&self, state: &State) {
        println!("state={:?}", state);
        let mut own_state = self.state.lock().unwrap();
        *own_state = (*state).clone();
    }

    fn on_call_rejected(&self) {
        self.rejected_calls.fetch_add(1, Ordering::SeqCst);
    }
}

trait IntoDuration {
    fn seconds(self) -> Duration;
}

impl IntoDuration for u64 {
    fn seconds(self) -> Duration {
        Duration::from_secs(self)
    }
}

mod mock_clock {
    include!("../src/mock_clock.rs");
}
