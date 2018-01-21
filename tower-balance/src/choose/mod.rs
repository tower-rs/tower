use ordermap::OrderMap;

use Loaded;

mod p2c;
mod round_robin;

pub use self::p2c::PowerOfTwoChoices;
pub use self::round_robin::RoundRobin;

/// A strategy for selecting nodes.
pub trait Choose {
    /// Returns the index of a ready endpoint.
    ///
    /// ## Panics
    ///
    /// If `ready` is empty.
    fn call<K, L: Loaded>(&mut self, ready: &OrderMap<K, L>) -> usize;
}
