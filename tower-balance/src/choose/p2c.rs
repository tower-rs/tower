use ordermap::OrderMap;
use rand::Rng;
use std::hash::Hash;
use std::marker::PhantomData;

use {Loaded,  Choose};

/// Chooses nodes by choosing the lesser-loaded of pairs of randomly-selected nodes.
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
}

impl<K, L, R> Choose for PowerOfTwoChoices<K, L, R>
where
    K: Hash + Eq,
    L: Loaded,
    R: Rng,
{
    type Key = K;
    type Loaded = L;


    fn call<'s>(&mut self, ready: &'s OrderMap<Self::Key, Self::Loaded>) -> &'s Self::Key {
        assert!(!ready.is_empty(), "must select over empty nodess");

        let (key0, load0) = {
            let i = self.rng.gen::<usize>() % ready.len();
            let (k, s) = ready.get_index(i).expect("out of bounds");
            (k, s.load())
        };

        let (key1, load1) = {
            let i = self.rng.gen::<usize>() % ready.len();
            let (k, s) = ready.get_index(i).expect("out of bounds");
            (k, s.load())
        };

        if load0 <= load1 {
            key0
        }  else {
            key1
        }
    }
}
