use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio_executor::enter;
use tokio_timer::clock::{self, Now};

#[derive(Clone, Debug)]
pub struct MockClock(pub(crate) Arc<Mutex<Instant>>);

impl Now for MockClock {
    fn now(&self) -> Instant {
        let clock = self.0.lock().unwrap();
        clock.clone()
    }
}

impl MockClock {
    pub fn now() -> MockClock {
        MockClock(Arc::new(Mutex::new(Instant::now())))
    }

    pub fn advance(&self, diff: Duration) {
        let mut clock = self.0.lock().unwrap();
        *clock = *clock + diff
    }
}

pub fn freeze<F, R>(f: F) -> R
where
    F: FnOnce(MockClock) -> R,
{
    let mock = MockClock::now();
    let clock = clock::Clock::new_with_now(mock.clone());
    let mut enter = enter().unwrap();

    clock::with_default(&clock, &mut enter, move |_| f(mock))
}
