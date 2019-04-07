use rand::{rngs::SmallRng, FromEntropy, Rng};

use crate::choose::{Choose, Replicas};
use crate::Load;

/// Chooses nodes using the [Power of Two Choices][p2c].
///
/// This is a load-aware strategy, so this may only be used to choose over services that
/// implement `Load`.
///
/// As described in the [Finagle Guide][finagle]:
/// > The algorithm randomly picks two nodes from the set of ready endpoints and selects
/// > the least loaded of the two. By repeatedly using this strategy, we can expect a
/// > manageable upper bound on the maximum load of any server.
/// >
/// > The maximum load variance between any two servers is bound by `ln(ln(n))` where `n`
/// > is the number of servers in the cluster.
///
/// [finagle]: https://twitter.github.io/finagle/guide/Clients.html#power-of-two-choices-p2c-least-loaded
/// [p2c]: http://www.eecs.harvard.edu/~michaelm/postscripts/handbook2001.pdf
#[derive(Debug)]
pub struct PowerOfTwoChoices {
    rng: SmallRng,
}

// ==== impl PowerOfTwoChoices ====

impl Default for PowerOfTwoChoices {
    fn default() -> Self {
        Self::new(SmallRng::from_entropy())
    }
}

impl PowerOfTwoChoices {
    pub fn new(rng: SmallRng) -> Self {
        Self { rng }
    }

    /// Returns two random, distinct indices into `ready`.
    fn random_pair(&mut self, len: usize) -> (usize, usize) {
        debug_assert!(len >= 2);

        // Choose a random number on [0, len-1].
        let idx0 = self.rng.gen::<usize>() % len;

        let idx1 = {
            // Choose a random number on [1, len-1].
            let delta = (self.rng.gen::<usize>() % (len - 1)) + 1;
            // Add it to `idx0` and then mod on `len` to produce a value on
            // [idx0+1, len-1] or [0, idx0-1].
            (idx0 + delta) % len
        };

        debug_assert!(idx0 != idx1, "random pair must be distinct");
        return (idx0, idx1);
    }
}

impl<K, L> Choose<K, L> for PowerOfTwoChoices
where
    L: Load,
    L::Metric: PartialOrd + ::std::fmt::Debug,
{
    /// Chooses two distinct nodes at random and compares their load.
    ///
    /// Returns the index of the lesser-loaded node.
    fn choose(&mut self, replicas: Replicas<K, L>) -> usize {
        let (a, b) = self.random_pair(replicas.len());

        let a_load = replicas[a].load();
        let b_load = replicas[b].load();
        trace!(
            "choose node[{a}]={a_load:?} node[{b}]={b_load:?}",
            a = a,
            b = b,
            a_load = a_load,
            b_load = b_load
        );
        if a_load <= b_load {
            a
        } else {
            b
        }
    }
}

#[cfg(test)]
mod tests {
    use quickcheck::*;

    use super::*;

    quickcheck! {
        fn distinct_random_pairs(n: usize) -> TestResult {
            if n < 2 {
                return TestResult::discard();
            }

            let mut p2c = PowerOfTwoChoices::default();

            let (a, b) = p2c.random_pair(n);
            TestResult::from_bool(a != b)
        }
    }
}
