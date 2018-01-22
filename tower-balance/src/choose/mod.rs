use ordermap::OrderMap;

use Load;

mod p2c;
mod round_robin;

pub use self::p2c::PowerOfTwoChoices;
pub use self::round_robin::RoundRobin;

/// A strategy for choosing nodes.
pub trait Choose {
    type Metric: PartialOrd;

    /// Returns the index of a ready endpoint.
    ///
    /// ## Panics
    ///
    /// If `nodes` is empty.
    fn call<K, L>(&mut self, nodes: &OrderMap<K, L>) -> usize
    where
        L: Load<Metric = Self::Metric>;
}
