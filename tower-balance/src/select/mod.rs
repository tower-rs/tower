use futures::Poll;
use ordermap::IterMut;
use std::hash::Hash;
use tower::Service;

use PollLoad;

mod p2c;
mod round_robin;

pub use self::p2c::PowerOfTwoChoices;
pub use self::round_robin::RoundRobin;

/// A strategy for selecting nodes.
pub trait Select {
    type Key: Hash + Eq;
    type Service: PollLoad;

    /// Returns the key of a ready endpoint.
    ///
    /// ## Panics
    ///
    /// If `endpoints` is empty.
    fn poll_next_ready<'s>(
        &mut self,
        nodes: IterMut<'s, Self::Key, Self::Service>
    ) -> Poll<&'s Self::Key, <Self::Service as Service>::Error>;
}
