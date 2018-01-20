use ordermap::OrderMap;
use std::hash::Hash;
use std::marker::PhantomData;

use {Loaded,  Choose};

/// Chooses nodes sequentially.
///
/// Note that ordering is not strictly enforced, especially when nodes are removed.
pub struct RoundRobin<K, L>
where
    K: Hash + Eq,
    L: Loaded,
{
    /// References the index of the next node to be polled.
    pos: usize,
    _p: PhantomData<(K, L)>,
}

impl<K, L> Default for RoundRobin<K, L>
where
    K: Hash + Eq,
    L: Loaded,
{
    fn default() -> Self {
        Self {
            pos: 0,
            _p: PhantomData,
        }
    }
}

impl<K, L> Choose for RoundRobin<K, L>
where
    K: Hash + Eq,
    L: Loaded,
{
    type Key = K;
    type Loaded = L;

    fn call<'s>(&mut self, ready: &'s OrderMap<K, L>) -> &'s Self::Key {
        assert!(!ready.is_empty(), "call be called with a non-empty set of endpoints");

        let len = ready.len();

        let idx = self.pos % len;
        self.pos = (idx + 1) % len;

        let (ref key, _) = ready.get_index(idx).expect("out of bounds");
        key
    }
}
