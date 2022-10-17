//! A retry "budget" for allowing only a certain amount of retries over time.
//!
//! # Why budgets and not max retries?
//!
//! The most common way of configuring retries is to specify a maximum
//! number of retry attempts to perform before giving up. This is a familiar idea to anyone
//! who’s used a web browser: you try to load a webpage, and if it doesn’t load, you try again.
//! If it still doesn’t load, you try a third time. Finally you give up.
//!
//! Unfortunately, there are at least two problems with configuring retries this way:
//!
//! **Choosing the maximum number of retry attempts is a guessing game.**
//! You need to pick a number that’s high enough to make a difference when things are somewhat failing,
//! but not so high that it generates extra load on the system when it’s really failing. In practice,
//! you usually pick a maximum retry attempts number out of a hat (e.g. 3) and hope for the best.
//!
//! **Systems configured this way are vulnerable to retry storms.**
//! A retry storm begins when one service starts to experience a larger than normal failure rate.
//! This causes its clients to retry those failed requests. The extra load from the retries causes the
//! service to slow down further and fail more requests, triggering more retries. If each client is
//! configured to retry up to 3 times, this can quadruple the number of requests being sent! To make
//! matters even worse, if any of the clients’ clients are configured with retries, the number of retries
//! compounds multiplicatively and can turn a small number of errors into a self-inflicted denial of service attack.
//!
//! It's generally dangerous to implement retries without some limiting factor. [`Budget`]s are that limit.
//!
//! # Examples
//!
//! ```rust
//! use std::sync::Arc;
//!
//! use futures_util::future;
//! use tower::retry::{budget::Budget, Policy};
//!
//! type Req = String;
//! type Res = String;
//!
//! #[derive(Clone, Debug)]
//! struct RetryPolicy {
//!     budget: Arc<Budget>,
//! }
//!
//! impl<E> Policy<Req, Res, E> for RetryPolicy {
//!     type Future = future::Ready<()>;
//!
//!     fn retry(&mut self, req: &mut Req, result: &mut Result<Res, E>) -> Option<Self::Future> {
//!         match result {
//!             Ok(_) => {
//!                 // Treat all `Response`s as success,
//!                 // so deposit budget and don't retry...
//!                 self.budget.deposit();
//!                 None
//!             }
//!             Err(_) => {
//!                 // Treat all errors as failures...
//!                 // Withdraw the budget, don't retry if we overdrew.
//!                 let withdrew = self.budget.withdraw().is_ok();
//!                 if !withdrew {
//!                     return None;
//!                 }
//!
//!                 // Try again!
//!                 Some(future::ready(()))
//!             }
//!         }
//!     }
//!
//!     fn clone_request(&mut self, req: &Req) -> Option<Req> {
//!         Some(req.clone())
//!     }
//! }
//! ```

use std::{
    fmt,
    sync::{
        atomic::{AtomicIsize, AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};
use tokio::time::Instant;

/// Represents a "budget" for retrying requests.
///
/// This is useful for limiting the amount of retries a service can perform
/// over a period of time, or per a certain number of requests attempted.
///
/// For more info about [`Budget`], please see the [module-level documentation].
///
/// [module-level documentation]: self
pub struct Budget<B = TpsBucket> {
    bucket: B,
    deposit_amount: isize,
    withdraw_amount: isize,
}

/// A [`Bucket`] for managing retry tokens.
/// 
/// [`SimpleBucket`] works by increasing the cost of retries on each withdraw.
/// Cost multiplier decrease by 1 on each deposit.
/// 
/// [`Budget`] uses a token bucket to decide if the request should be retried.
/// 
/// For more info about [`Budget`], please see the [module-level documentation].
/// For more info about [`Bucket`], see [bucket trait]
/// 
/// [module-level documentation]: self
/// [bucket trait]: self::Bucket
#[derive(Debug)]
pub struct SimpleBucket {
    /// Initial and max budget the bucket can have in reserve
    max_budget: isize,
    // Amount to add to multiplier for each withdraw
    step_count: usize,
    // Current budget
    reserve: AtomicIsize,
    // The amount of cost of the next retry
    multiplier: AtomicUsize,
}

/// A [`Bucket`] for managing retry tokens.
/// 
/// [`Budget`] uses a token bucket to decide if the request should be retried.
/// 
/// [`TpsBucket`] works by checking how much retries have been made in a certain period of time.
/// Minimum allowed number of retries are effectively reset on an interval. Allowed number of
/// retries depends on failed request count in recent time frame.
/// 
/// For more info about [`Budget`], please see the [module-level documentation].
/// For more info about [`Bucket`], see [bucket trait]
/// 
/// [module-level documentation]: self
/// [bucket trait]: self::Bucket
#[derive(Debug)]
pub struct TpsBucket {
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

/// For more info about [`Budget`], please see the [module-level documentation].
///
/// [module-level documentation]: self
pub trait BudgetTrait {
    /// Store a "deposit" in the budget, which will be used to permit future
    /// withdrawals.
    fn deposit(&self);

    /// Check whether there is enough "balance" in the budget to issue a new
    /// retry.
    ///
    /// If there is not enough, false is returned.
    fn withdraw(&self) -> bool;
}

/// Represents a token bucket.
/// 
/// A token bucket manages a reserve of tokens to decide if a retry for the request is
/// possible. Successful requests put tokens into the reserve. Before a request is retried,
/// bucket is checked to ensure there are sufficient amount of tokens available. If there are,
/// specified amount of tokens are withdrawn.
/// 
/// For more info about [`Budget`], please see the [module-level documentation].
/// 
/// [module-level documentation]: self
pub trait Bucket {
    /// Deposit `amt` of tokens into the bucket.
    fn put(&self, amt: isize);
    /// Try to withdraw `amt` of tokens from bucket. If reserve do not have sufficient
    /// amount of tokens false is returned. If withdraw is possible, decreases the reserve 
    /// and true is returned.
    fn try_get(&self, amt: isize) -> bool;
    /// Returns the amount of tokens in the reserve.
    fn reserve(&self) -> isize;
}

// ===== impl Budget =====

impl Budget {
    /// Create a [`Budget`] that allows for a certain percent of the total
    /// requests to be retried.
    ///
    /// - The `retry_budget` is the minimum number of retries allowed
    ///   to accomodate clients that have just started issuing requests. If the
    ///   `step_count` is 0, `retry_budget` is the max number of consecutive retries
    ///   allowed.
    /// - The `step_count` is the limiting factor for consecutive retries. Each consecutive
    ///   retry costs `step_count` more to execute and each successful request gradually
    ///   reduces the cost. Must be below 10.
    ///
    ///   For example, if `2` is used for `step_count` each consecutive retry will cost
    ///   2 more (1, 3, 5, 7...). Each successful request will reduce the cost by 1.
    ///   If 0 is used for `step_count` retry cost will be same for all retries.
    ///
    /// - The `retry_ratio` is the percentage of calls to `deposit` that can
    ///   be retried. This is in addition to any retries allowed for via
    ///   `retry_budget`. Must be between 0 and 1000.
    ///
    ///   As an example, if `0.1` is used, then for every 10 calls to `deposit`,
    ///   1 retry will be allowed. If `2.0` is used, then every `deposit`
    ///   allows for 2 retries.
    pub fn new(retry_budget: u32, step_count: u32, retry_ratio: f32) -> Budget<SimpleBucket> {
        assert!(retry_ratio > 0.0);
        assert!(retry_ratio <= 1000.0);
        assert!(retry_budget < ::std::i32::MAX as u32);
        assert!(step_count <= 10);

        let (deposit_amount, withdraw_amount) = if retry_ratio <= 1.0 {
            (1, (1.0 / retry_ratio) as isize)
        } else {
            (1000, (1000.0 / retry_ratio) as isize)
        };

        let reserve = (retry_budget as isize).saturating_mul(withdraw_amount);

        Budget {
            bucket: SimpleBucket {
                max_budget: reserve,
                step_count: step_count as usize,
                reserve: AtomicIsize::new(reserve),
                multiplier: AtomicUsize::new(1),
            },
            deposit_amount,
            withdraw_amount,
        }
    }

    /// Create a [`Budget`] that allows for a certain percent of the total
    /// requests to be retried.
    ///
    /// - The `ttl` is the duration of how long a single `deposit` should be
    ///   considered. Must be between 1 and 60 seconds.
    /// - The `min_per_sec` is the minimum rate of retries allowed to accommodate
    ///   clients that have just started issuing requests, or clients that do
    ///   not issue many requests per window.
    /// - The `retry_percent` is the percentage of calls to `deposit` that can
    ///   be retried. This is in addition to any retries allowed for via
    ///   `min_per_sec`. Must be between 0 and 1000.
    ///
    ///   As an example, if `0.1` is used, then for every 10 calls to `deposit`,
    ///   1 retry will be allowed. If `2.0` is used, then every `deposit`
    ///   allows for 2 retries.
    pub fn new_tps(ttl: Duration, min_per_sec: u32, retry_percent: f32) -> Budget<TpsBucket> {
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
            // meaning for every deposit D, D * retry_percent withdrawals
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
            bucket: TpsBucket {
                generation: Mutex::new(Generation {
                    index: 0,
                    time: Instant::now(),
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
}

impl<B: Bucket> BudgetTrait for Budget<B> {
    fn deposit(&self) {
        self.bucket.put(self.deposit_amount)
    }

    fn withdraw(&self) -> bool {
        self.bucket.try_get(self.withdraw_amount)
    }
}

impl Default for Budget {
    fn default() -> Self {
        Budget::new_tps(Duration::from_secs(10), 10, 0.2)
    }
}

impl fmt::Debug for Budget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Budget")
            .field("deposit", &self.deposit_amount)
            .field("withdraw", &self.withdraw_amount)
            .field("balance", &self.bucket.reserve())
            .finish()
    }
}

// ===== impl Bucket =====

impl TpsBucket {
    fn expire(&self) {
        let mut gen = self.generation.lock().expect("generation lock");

        let now = Instant::now();
        let diff = now.saturating_duration_since(gen.time);
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

impl Bucket for TpsBucket {
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

    fn reserve(&self) -> isize {
        self.sum()
    }
}

impl Bucket for SimpleBucket {
    fn put(&self, amt: isize) {
        self.reserve.fetch_add(amt, Ordering::SeqCst);
        self.reserve.fetch_min(self.max_budget, Ordering::SeqCst);

        if self.multiplier.load(Ordering::SeqCst) > 1 {
            self.multiplier.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn try_get(&self, amt: isize) -> bool {
        debug_assert!(amt >= 0);

        let multiplier = self.multiplier.load(Ordering::SeqCst);
        let withdraw_amt = amt.saturating_mul(multiplier as isize);

        if self.reserve.load(Ordering::SeqCst) >= withdraw_amt {
            self.reserve.fetch_add(-withdraw_amt, Ordering::SeqCst);
            self.multiplier.fetch_add(self.step_count, Ordering::SeqCst);
            self.multiplier
                .fetch_min((self.step_count * 10) + 1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    fn reserve(&self) -> isize {
        self.reserve.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time;

    #[test]
    fn simple_empty() {
        let bgt = Budget::new(0, 0, 1.0);
        assert!(!bgt.withdraw())
    }

    #[test]
    fn simple_reserve() {
        let bgt = Budget::new(5, 0, 1.0);
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());

        assert!(!bgt.withdraw());
    }

    #[test]
    fn simple_max_reserve() {
        let bgt = Budget::new(2, 0, 1.0);
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(!bgt.withdraw());

        // Reserve should not exceed 2
        bgt.deposit();
        bgt.deposit();
        bgt.deposit();

        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(!bgt.withdraw());
    }

    #[test]
    fn simple_step() {
        let bgt = Budget::new(5, 2, 1.0);
        assert!(bgt.withdraw()); // Next cost 3
        assert!(bgt.withdraw()); // Next cost 5

        // Each consecutive retry should cost 2 more 1 + 3 = 4
        assert!(!bgt.withdraw());

        // Each deposit should reduce the cost by 1
        bgt.deposit();
        bgt.deposit();
        assert!(bgt.withdraw());

        assert!(!bgt.withdraw());
    }

    #[test]
    fn tps_empty() {
        let bgt = Budget::new_tps(Duration::from_secs(1), 0, 1.0);
        assert!(!bgt.withdraw());
    }

    #[tokio::test]
    async fn tps_leaky() {
        time::pause();

        let bgt = Budget::new_tps(Duration::from_secs(1), 0, 1.0);
        bgt.deposit();

        time::advance(Duration::from_secs(3)).await;

        assert!(!bgt.withdraw());
    }

    #[tokio::test]
    async fn tps_slots() {
        time::pause();

        let bgt = Budget::new_tps(Duration::from_secs(1), 0, 0.5);
        bgt.deposit();
        bgt.deposit();
        time::advance(Duration::from_millis(901)).await;
        // 900ms later, the deposit should still be valid
        assert!(bgt.withdraw());

        // blank slate
        time::advance(Duration::from_millis(2001)).await;

        bgt.deposit();
        time::advance(Duration::from_millis(301)).await;
        bgt.deposit();
        time::advance(Duration::from_millis(801)).await;
        bgt.deposit();

        // the first deposit is expired, but the 2nd should still be valid,
        // combining with the 3rd
        assert!(bgt.withdraw());
    }

    #[tokio::test]
    async fn tps_reserve() {
        let bgt = Budget::new_tps(Duration::from_secs(1), 5, 1.0);
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());

        assert!(!bgt.withdraw());
    }
}
