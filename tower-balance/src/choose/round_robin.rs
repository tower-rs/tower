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
        assert!(2 <= ready.len(), "must choose over 2 or more ready nodes");

        let len = ready.len();
        let idx = self.pos % len;
        self.pos = (idx + 1) % len;
        idx
    }
}
