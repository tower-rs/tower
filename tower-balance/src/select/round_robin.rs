use futures::{Async, Poll};
use ordermap::IterMut;
use std::hash::Hash;
use std::marker::PhantomData;

use {PollLoad,  Select};

/// Selects nodes sequentially.
///
/// Note that ordering is not strictly enforced, especially when nodes are removed.
pub struct RoundRobin<K, S>
where
    K: Hash + Eq,
    S: PollLoad,
{
    /// References the index of the next node to be polled.
    pos: usize,
    _p: PhantomData<(K, S)>,
}

impl<K, S> Default for RoundRobin<K, S>
where
    K: Hash + Eq,
    S: PollLoad,
{
    fn default() -> Self {
        Self {
            pos: 0,
            _p: PhantomData,
        }
    }
}

impl<K, S> Select for RoundRobin<K, S>
where
    K: Hash + Eq,
    S: PollLoad,
{
    type Key = K;
    type Service = S;

    fn poll_next_ready<'s>(&mut self, iter: IterMut<'s, K, S>) -> Poll<&'s Self::Key, S::Error> {
        let mut nodes = iter.collect::<Vec<_>>();
        assert!(!nodes.is_empty(), "poll_next_ready be called with a non-empty set of endpoints");

        let len = nodes.len();
        for _ in 0..len {
            let idx = self.pos % len;
            self.pos = (idx + 1) % len;

            let ready = {
                let (_, ref mut svc) = nodes[idx];
                svc.poll_load()?.is_ready()
            };

            if ready {
                let (ref key, _) = nodes[idx];
                return Ok(Async::Ready(key));
            }
        }

        Ok(Async::NotReady)
    }
}
