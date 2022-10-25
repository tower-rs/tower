//! [`BasicBudget`] implementations

use std::{
    fmt,
    sync::atomic::{AtomicIsize, AtomicUsize, Ordering},
};

use super::Budget;

/// A [`Budget`] for managing retry tokens.
///
/// [`BasicBudget`] works by increasing the cost of retries on each withdraw.
/// Cost multiplier decrease by 1 on each deposit.
///
/// For more info about [`Budget`], please see the [module-level documentation].
///
/// [module-level documentation]: super
pub struct BasicBudget {
    /// Initial and max budget the bucket can have in reserve
    max_budget: isize,
    // Amount to add to multiplier for each withdraw
    step_count: usize,
    // Current budget
    reserve: AtomicIsize,
    // The amount of cost of the next retry
    multiplier: AtomicUsize,
    /// Amount of tokens to deposit for each put().
    deposit_amount: isize,
    /// Amount of tokens to withdraw for each try_get().
    withdraw_amount: isize,
}

// ===== impl BasicBudget =====

impl BasicBudget {
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
    pub fn new(retry_budget: u32, step_count: u32, retry_ratio: f32) -> Self {
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

        BasicBudget {
            max_budget: reserve,
            step_count: step_count as usize,
            reserve: AtomicIsize::new(reserve),
            multiplier: AtomicUsize::new(1),
            deposit_amount,
            withdraw_amount,
        }
    }

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

impl Default for BasicBudget {
    fn default() -> Self {
        BasicBudget::new(100, 0, 0.2)
    }
}

impl fmt::Debug for BasicBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Budget")
            .field("deposit", &self.deposit_amount)
            .field("withdraw", &self.withdraw_amount)
            .field("balance", &self.reserve())
            .finish()
    }
}

impl Budget for BasicBudget {
    fn deposit(&self) {
        self.put(self.deposit_amount)
    }

    fn withdraw(&self) -> bool {
        self.try_get(self.withdraw_amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_empty() {
        let bgt = BasicBudget::new(0, 0, 1.0);
        assert!(!bgt.withdraw())
    }

    #[test]
    fn simple_reserve() {
        let bgt = BasicBudget::new(5, 0, 1.0);
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());
        assert!(bgt.withdraw());

        assert!(!bgt.withdraw());
    }

    #[test]
    fn simple_max_reserve() {
        let bgt = BasicBudget::new(2, 0, 1.0);
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
        let bgt = BasicBudget::new(5, 2, 1.0);
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
}
