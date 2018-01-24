use choose::{Choose, Replicas};

/// Chooses nodes sequentially.
///
/// This strategy is load-agnostic and may therefore be used to choose over any type of
/// service.
///
/// Note that ordering is not strictly enforced, especially when services are removed by
/// the balancer.
#[derive(Debug, Default)]
pub struct RoundRobin {
    /// References the index of the next node to be used.
    pos: usize,
}

impl<K, N> Choose<K, N> for RoundRobin {
    fn choose(&mut self, nodes: Replicas<K, N>) -> usize {
        let len = nodes.len();
        let idx = self.pos % len;
        self.pos = (idx + 1) % len;
        idx
    }
}
