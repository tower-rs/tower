use ordermap::OrderMap;

use {Load,  Choose};

/// Chooses nodes sequentially.
///
/// Note that ordering is not strictly enforced, especially when nodes are removed.
#[derive(Debug, Default)]
pub struct RoundRobin {
    /// References the index of the next node to be polled.
    pos: usize,
}

impl Choose for RoundRobin {
    type Metric = ();

    fn call<K, L: Load>(&mut self, ready: &OrderMap<K, L>) -> usize {
        let len = ready.len();
        assert!(2 <= len, "must choose over 2 or more ready nodes");

        let idx = self.pos % len;
        self.pos = (idx + 1) % len;
        idx
    }
}
