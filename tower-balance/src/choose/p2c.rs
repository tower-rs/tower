use ordermap::OrderMap;
use rand::Rng;

use {Load, Loaded,  Choose};

/// Chooses nodes using the [Power of Two Choices][p2c].
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

    fn random_pair<K, L: Loaded>(&mut self, ready: &OrderMap<K, L>) -> (usize, usize) {
        assert!(2 <= ready.len(), "must choose over 2 or more ready nodes");

        if ready.len() == 2 {
            // Avoid looping if there are only two nodes.
            if self.rng.gen::<bool>() {
                return (0, 1);
            } else {
                return (1, 0);
            }
        }

        let idx0 = self.rng.gen::<usize>() % ready.len();
        loop {
            let idx1 = self.rng.gen::<usize>() % ready.len();
            if idx0 != idx1 {
                return (idx0, idx1);
            }
        }
    }

    fn get_load<K, L: Loaded>(ready: &OrderMap<K, L>, idx: usize) -> Load {
        let (_, s) = ready.get_index(idx).expect("out of bounds");
        s.load()
    }
}

impl<R: Rng> Choose for PowerOfTwoChoices<R> {
    /// Chooses two distinct nodes at random and compares their load.
    ///
    /// Returns the index of the lesser-loaded node.
    fn call<K, L: Loaded>(&mut self, ready: &OrderMap<K, L>) -> usize {
        let (idx0, idx1) = self.random_pair(ready);
        if Self::get_load(ready, idx0) <= Self::get_load(ready, idx1) {
            return idx0;
        } else {
            return idx1;
        }
    }
}
