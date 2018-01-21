use ordermap::OrderMap;

use Loaded;

mod p2c;
mod round_robin;

pub use self::p2c::PowerOfTwoChoices;
pub use self::round_robin::RoundRobin;

/// A strategy for choosing nodes.
pub trait Choose {

    /// Returns the index of a ready endpoint.
    ///
    /// ## Panics
    ///
    /// If `nodes` is empty.
    fn call<K, L: Loaded>(&mut self, nodes: &OrderMap<K, L>) -> usize;
}
