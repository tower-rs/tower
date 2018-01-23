use rand::Rng;

use Load;
use choose::{Choose, Replicas};

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
pub struct PowerOfTwoChoices<R> {
    rng: R,
}

impl<R: Rng> PowerOfTwoChoices<R> {
    pub fn new(rng: R) -> Self {
        Self { rng }
    }

    /// Returns two random, distinct indices into `ready`.
    fn random_pair(&mut self, len: usize) -> (usize, usize) {
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

impl<K, L, R> Choose<K, L> for PowerOfTwoChoices<R>
where
    L: Load,
    L::Metric: PartialOrd,
    R: Rng,
{
    /// Chooses two distinct nodes at random and compares their load.
    ///
    /// Returns the index of the lesser-loaded node.
    fn choose(&mut self, replicas: Replicas<K, L>) -> usize {
        let (a, b) = self.random_pair(replicas.len());
        if replicas[a].load() <= replicas[b].load() {
            a
        } else {
            b
        }
    }
}
