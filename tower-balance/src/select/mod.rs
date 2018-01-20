use ordermap::OrderMap;
use std::hash::Hash;

use Loaded;

mod p2c;
mod round_robin;

pub use self::p2c::PowerOfTwoChoices;
pub use self::round_robin::RoundRobin;

/// A strategy for selecting nodes.
pub trait Select {
    type Key: Hash + Eq;
    type Loaded: Loaded;

    /// Returns the key of a ready endpoint.
    ///
    /// ## Panics
    ///
    /// If `ready` is empty.
    fn call<'s>(&mut self, ready: &'s OrderMap<Self::Key, Self::Loaded>) -> &'s Self::Key;
}
