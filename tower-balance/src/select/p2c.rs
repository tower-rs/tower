use futures::{Async, Poll};
use ordermap::IterMut;
use rand::Rng;
use std::hash::Hash;
use std::marker::PhantomData;

use {PollLoad,  Select};

/// Selects nodes by choosing the lesser-loaded of pairs of randomly-selected nodes.
pub struct PowerOfTwoChoices<K, S, R>
where
    K: Hash + Eq,
    S: PollLoad,
    R: Rng,
{
    rng: R,
    _p: PhantomData<(K, S)>,
}

impl<K, S, R: Rng> PowerOfTwoChoices<K, S, R>
where
    K: Hash + Eq,
    S: PollLoad,
    R: Rng,
{
    pub fn new(rng: R) -> Self {
        Self { rng, _p: PhantomData }
    }
}

impl<K, S, R> Select for PowerOfTwoChoices<K, S, R>
where
    K: Hash + Eq,
    S: PollLoad,
    R: Rng,
{
    type Key = K;
    type Service = S;

    fn poll_next_ready<'s>(&mut self, iter: IterMut<'s, K, S>) -> Poll<&'s Self::Key, S::Error> {
        let mut nodes = iter.collect::<Vec<_>>();
        assert!(!nodes.is_empty(), "must select over empty nodess");

        // Randomly select pairs of nodes to compare until a ready node is found. If both
        // nodes are ready, the lesser-loaded endpoint is used.
        while !nodes.is_empty() {
            if nodes.len() == 1 {
                // If only one node is present, ensure that it's ready before returning it.
                let (key, svc) = nodes.pop().unwrap();
                return Ok(svc.poll_load()?.map(|_| key));
            }

            let (key0, load0) = {
                let i = self.rng.gen::<usize>() % nodes.len();
                let (k, s) = nodes.swap_remove(i);
                (k, s.poll_load()?)
            };

            let (key1, load1) = {
                let i = self.rng.gen::<usize>() % nodes.len();
                let (k, s) = nodes.swap_remove(i);
                (k, s.poll_load()?)
            };

            let key = match (load0, load1) {
                (Async::NotReady, Async::NotReady) => {
                    // Neither node is ready, so continue to inspect additional nodes.
                    continue;
                }
                (Async::Ready(_), Async::NotReady) => key0,
                (Async::NotReady, Async::Ready(_)) => key1,
                (Async::Ready(l0), Async::Ready(l1)) => if l0 < l1 { key0 } else { key1 },
            };

            return Ok(Async::Ready(key));
        }

        Ok(Async::NotReady)
    }
}
