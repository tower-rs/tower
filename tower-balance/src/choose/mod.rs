use ordermap::OrderMap;

mod p2c;
mod round_robin;

pub use self::p2c::PowerOfTwoChoices;
pub use self::round_robin::RoundRobin;

/// A strategy for choosing nodes.
// TODO hide `K`
pub trait Choose<K, N> {
    /// Returns the index of a replica to be used next.
    ///
    /// `replicas` cannot be empty, so this function must always return a valid index on
    /// [0, replicas.len()-1].
    fn choose(&mut self, replicas: Replicas<K, N>) -> usize;
}

/// Creates a `Replicas` if there are two or more services.
///
pub(crate) fn replicas<K, S>(inner: &OrderMap<K, S>) -> Result<Replicas<K, S>, TooFew> {
    if inner.len() < 2 {
        return Err(TooFew);
    }

    Ok(Replicas(inner))
}

/// Indicates that there were not at least two services.
#[derive(Copy, Clone, Debug)]
pub struct TooFew;

/// Holds two or more services.
// TODO hide `K`
pub struct Replicas<'a, K: 'a, S: 'a>(&'a OrderMap<K, S>);

impl<'a, K: 'a, S: 'a> Replicas<'a, K, S> {
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, K: 'a, S: 'a> ::std::ops::Index<usize> for Replicas<'a, K, S> {
    type Output = S;

    fn index(&self, idx: usize) -> &Self::Output {
        let (_, service) = self.0.get_index(idx).expect("out of bounds");
        service
    }
}
