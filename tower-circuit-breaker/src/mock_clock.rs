use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio_timer::clock::{self, Now};
use tokio_timer::{timer, Timer};

use tokio_executor::enter;
use tokio_executor::park::ParkThread;

pub trait IntoDuration {
    fn seconds(self) -> Duration;
}

impl IntoDuration for u64 {
    fn seconds(self) -> Duration {
        Duration::from_secs(self)
    }
}

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

#[derive(Debug)]
pub struct MockTimer {
    inner: Arc<Mutex<Timer<ParkThread>>>,
    clock: MockClock,
}

impl MockTimer {
    pub fn advance(&self, diff: Duration) {
        self.clock.advance(diff);
        self.turn();
    }

    pub fn turn(&self) {
        let mut timer = self.inner.lock().unwrap();
        timer.turn(Some(Duration::from_millis(10))).unwrap();
    }
}

pub fn freeze<F, R>(f: F) -> R
where
    F: FnOnce(MockTimer) -> R,
{
    use futures::{future::lazy, Future};

    let mock_clock = MockClock::now();
    let clock = clock::Clock::new_with_now(mock_clock.clone());

    let park = ParkThread::new();
    let timer = Timer::new_with_now(park, clock.clone());
    let mut enter = enter().unwrap();

    clock::with_default(&clock, &mut enter, |mut enter| {
        let handle = timer.handle();
        timer::with_default(&handle, &mut enter, |_| {
            let future = lazy(|| {
                let timer = MockTimer {
                    inner: Arc::new(Mutex::new(timer)),
                    clock: mock_clock,
                };
                let ok = f(timer);
                Ok::<_, ()>(ok)
            });
            future.wait().unwrap()
        })
    })
}
