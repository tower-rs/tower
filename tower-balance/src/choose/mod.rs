use ordermap::OrderMap;

mod p2c;
mod round_robin;

pub use self::p2c::PowerOfTwoChoices;
pub use self::round_robin::RoundRobin;

/// A strategy for choosing nodes.
pub trait Choose<K, N> {
    /// Returns the index of a ready endpoint.
    ///
    /// ## Panics
    ///
    /// If `nodes` has fewer than 2 entries.
    fn choose(&mut self, nodes: Nodes<K, N>) -> usize;
}

/// Holds two or more loaded nodes.
pub struct Nodes<'a, K: 'a, N: 'a>(&'a OrderMap<K, N>);

impl<'a, K: 'a, N: 'a> Nodes<'a, K, N> {
    pub(crate) fn new(inner: &'a OrderMap<K, N>) -> Self {
        assert!(2 <= inner.len(), "Nodes must have 2 or more items");
        Nodes(inner)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, K: 'a, N: 'a> ::std::ops::Index<usize> for Nodes<'a, K, N> {
    type Output = N;

    fn index(&self, idx: usize) -> &Self::Output {
        let (_, node) = self.0.get_index(idx).expect("out of bounds");
        node
    }
}
