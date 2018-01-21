use ordermap::OrderMap;
use rand::Rng;
use std::hash::Hash;
use std::marker::PhantomData;

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
pub struct PowerOfTwoChoices<K, L, R>
where
    K: Hash + Eq,
    L: Loaded,
    R: Rng,
{
    rng: R,
    _p: PhantomData<(K, L)>,
}

impl<K, L, R: Rng> PowerOfTwoChoices<K, L, R>
where
    K: Hash + Eq,
    L: Loaded,
    R: Rng,
{
    pub fn new(rng: R) -> Self {
        Self { rng, _p: PhantomData }
    }

    fn choose(&mut self, ready: &OrderMap<K, L>) -> (usize, Load) {
        let i = self.rng.gen::<usize>() % ready.len();
        let (_, s) = ready.get_index(i).expect("out of bounds");
        (i, s.load())
    }
}

impl<K, L, R> Choose for PowerOfTwoChoices<K, L, R>
where
    K: Hash + Eq,
    L: Loaded,
    R: Rng,
{
    type Key = K;
    type Loaded = L;

    /// Chooses two distinct nodes at random and compares their load.
    ///
    /// Returns the index of the lesser-loaded node.
    fn call(&mut self, ready: &OrderMap<K, L>) -> usize {
        assert!(2 <= ready.len(), "must choose over 2 or more ready nodes");

        let (idx0, load0) = self.choose(ready);
        loop {
            let (idx1, load1) = self.choose(ready);

            if idx0 == idx1 {
                continue;
            }

            if load0 <= load1 {
                return idx0;
            } else {
                return idx1;
            }
        }
    }
}
