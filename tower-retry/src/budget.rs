use std::fmt;
use std::sync::{Mutex, atomic::{AtomicIsize, Ordering}};
use std::time::{Duration, Instant};

use tokio_timer::clock;

pub struct Budget {
    bucket: Bucket,
    deposit_amount: isize,
    withdraw_amount: isize,
}

#[derive(Debug)]
pub struct Overdrawn {
    _inner: (),
}

#[derive(Debug)]
struct Bucket {
    generation: Mutex<Generation>,
    /// Initial budget allowed for every second.
    reserve: isize,
    /// Slots of a the TTL divided evenly.
    slots: Box<[AtomicIsize]>,
    /// The amount of time represented by each slot.
    window: Duration,
    /// The changers for the current slot to be commited
    /// after the slot expires.
    writer: AtomicIsize,
}

#[derive(Debug)]
struct Generation {
    /// Slot index of the last generation.
    index: usize,
    /// The timestamp since the last generation expired.
    time: Instant,
}

// ===== impl Budget =====

impl Budget {
    pub fn new(ttl: Duration, min_per_sec: u32, retry_percent: f32) -> Self {
        // assertions taken from finagle
        assert!(ttl >= Duration::from_secs(1));
        assert!(ttl <= Duration::from_secs(60));
        assert!(retry_percent >= 0.0);
        assert!(retry_percent <= 1000.0);
        assert!(min_per_sec < ::std::i32::MAX as u32);


        let (deposit_amount, withdraw_amount) = if retry_percent == 0.0 {
            // If there is no percent, then you gain nothing from deposits.
            // Withdrawals can only be made against the reserve, over time.
            (0, 1)
        } else if retry_percent <= 1.0 {
            (1, (1.0 / retry_percent) as isize)
        } else {
            // Support for when retry_percent is between 1.0 and 1000.0,
            // meaning for every deposit D, D*retry_percent withdrawals
            // can be made.
            (1000, (1000.0 / retry_percent) as isize)
        };
        let reserve = (min_per_sec as isize)
            .saturating_mul(ttl.as_secs() as isize) // ttl is between 1 and 60 seconds
            .saturating_mul(withdraw_amount);

        // AtomicIsize isn't clone, so the slots need to be built in a loop...
        let windows = 10u32;
        let mut slots = Vec::with_capacity(windows as usize);
        for _ in 0..windows {
            slots.push(AtomicIsize::new(0));
        }

        Budget {
            bucket: Bucket {
                generation: Mutex::new(Generation {
                    index: 0,
                    time: clock::now(),
                }),
                reserve,
                slots: slots.into_boxed_slice(),
                window: ttl / windows,
                writer: AtomicIsize::new(0),
            },
            deposit_amount,
            withdraw_amount,
        }
    }

    pub fn deposit(&self) {
        self.bucket.put(self.deposit_amount);
    }

    pub fn withdraw(&self) -> Result<(), Overdrawn> {
        if self.bucket.try_get(self.withdraw_amount) {
            Ok(())
        } else {
            Err(Overdrawn {
                _inner: (),
            })
        }
    }
}

impl Default for Budget {
    fn default() -> Budget {
        Budget::new(Duration::from_secs(10), 10, 0.2)
    }
}

impl fmt::Debug for Budget {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Budget")
            .field("deposit", &self.deposit_amount)
            .field("withdraw", &self.withdraw_amount)
            .field("balance", &self.bucket.sum())
            .finish()
    }
}

// ===== impl Bucket =====

impl Bucket {
    fn put(&self, amt: isize) {
        self.expire();
        self.writer.fetch_add(amt, Ordering::SeqCst);
    }

    fn try_get(&self, amt: isize) -> bool {
        debug_assert!(amt >= 0);

        self.expire();

        let sum = self.sum();
        if sum >= amt {
            self.writer.fetch_add(-amt, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    fn expire(&self) {
        let mut gen = self
            .generation
            .lock()
            .expect("generation lock");

        let now = clock::now();
        let diff = now - gen.time;
        if diff < self.window {
            // not expired yet
            return;
        }

        let to_commit = self.writer.swap(0, Ordering::SeqCst);
        self.slots[gen.index].store(to_commit, Ordering::SeqCst);

        let mut diff = diff;
        let mut idx = (gen.index + 1) % self.slots.len();
        while diff > self.window {
            self.slots[idx].store(0, Ordering::SeqCst);
            diff -= self.window;
            idx = (idx + 1) % self.slots.len();
        }

        gen.index = idx;
        gen.time = now;
    }

    fn sum(&self) -> isize {
        let current = self.writer.load(Ordering::SeqCst);
        let windowed_sum: isize = self
            .slots
            .iter()
            .map(|slot| slot.load(Ordering::SeqCst))
            // fold() is used instead of sum() to determine overflow behavior
            .fold(0, isize::saturating_add);

        current
            .saturating_add(windowed_sum)
            .saturating_add(self.reserve)
    }
}

#[cfg(test)]
mod tests {
    extern crate tokio_executor;

    use std::sync::{Arc, Mutex, MutexGuard};
    use std::time::Instant;
    use super::*;
    use self::tokio_executor::enter;

    #[test]
    fn empty() {
        let bgt = Budget::new(Duration::from_secs(1), 0, 1.0);
        bgt.withdraw().unwrap_err();
    }

    #[test]
    fn leaky() {
        let time = MockNow(Arc::new(Mutex::new(Instant::now())));
        let clock = clock::Clock::new_with_now(time.clone());
        clock::with_default(&clock, &mut enter().unwrap(), |_| {
            let bgt = Budget::new(Duration::from_secs(1), 0, 1.0);
            bgt.deposit();

            *time.as_mut() += Duration::from_secs(3);

            bgt.withdraw().unwrap_err();
        });
    }

    #[test]
    fn slots() {
        let time = MockNow(Arc::new(Mutex::new(Instant::now())));
        let clock = clock::Clock::new_with_now(time.clone());
        clock::with_default(&clock, &mut enter().unwrap(), |_| {
            let bgt = Budget::new(Duration::from_secs(1), 0, 0.5);
            bgt.deposit();
            bgt.deposit();
            *time.as_mut() += Duration::from_millis(900);
            // 900ms later, the deposit should still be valid
            bgt.withdraw().unwrap();

            // blank slate
            *time.as_mut() += Duration::from_millis(2000);

            bgt.deposit();
            *time.as_mut() += Duration::from_millis(300);
            bgt.deposit();
            *time.as_mut() += Duration::from_millis(800);
            bgt.deposit();

            // the first deposit is expired, but the 2nd should still be valid,
            // combining with the 3rd
            bgt.withdraw().unwrap();
        });
    }

    #[test]
    fn reserve() {
        let time = MockNow(Arc::new(Mutex::new(Instant::now())));
        let clock = clock::Clock::new_with_now(time.clone());
        clock::with_default(&clock, &mut enter().unwrap(), |_| {
            let bgt = Budget::new(Duration::from_secs(1), 5, 1.0);
            bgt.withdraw().unwrap();
            bgt.withdraw().unwrap();
            bgt.withdraw().unwrap();
            bgt.withdraw().unwrap();
            bgt.withdraw().unwrap();

            bgt.withdraw().unwrap_err();
        });
    }

    #[derive(Clone)]
    struct MockNow(Arc<Mutex<Instant>>);

    impl MockNow {
        fn as_mut(&self) -> MutexGuard<Instant> {
            self.0.lock().unwrap()
        }
    }

    impl clock::Now for MockNow {
        fn now(&self) -> Instant {
            *self.0.lock().expect("now")
        }
    }
}
