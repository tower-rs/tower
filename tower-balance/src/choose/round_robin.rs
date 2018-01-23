use choose::{Choose, Nodes};

/// Chooses nodes sequentially.
///
/// Note that ordering is not strictly enforced, especially when nodes are removed.
#[derive(Debug, Default)]
pub struct RoundRobin {
    /// References the index of the next node to be polled.
    pos: usize,
}

impl<K, N> Choose<K, N> for RoundRobin {
    fn choose(&mut self, nodes: Nodes<K, N>) -> usize {
        let len = nodes.len();
        let idx = self.pos % len;
        self.pos = (idx + 1) % len;
        idx
    }
}
