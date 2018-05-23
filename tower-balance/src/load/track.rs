use futures::{Future, Poll};
use std::marker::PhantomData;

/// Attaches `T`-typed trackers to `V` typed values.
///
/// This utility allows load metrics to have a protocol-agnostic means to track streams
/// past their initial response future. For example, if `V` represents an HTTP response
/// type, an implementaton could add `T`-typed trackers to the HTTP response extensions.
///
/// Trackers are intended to be RAII guards that primarily implement `Drop` and update
/// load metric state as they are dropped.
///
/// A base `impl<T, V> Track<T, V> for ()` is provided that drops the tracker
/// immediately.
pub trait Track<T, V> {

    /// Attaches a `T`-typed trakcer to a `V`-typed value.
    fn track(tracker: T, item: &mut V);

    /// Wraps an `F`-typred Future so that a `T`-typed tracker is attached to its result.
    fn track_future<F>(tracker: T, future: F) -> TrackFuture<F, T, Self>
    where
        F: Future<Item = V>,
        Self: Sized,
    {
        TrackFuture {
            future,
            tracker: Some(tracker),
            _p: PhantomData,
        }
    }
}

/// Drops each tracker immediately.
#[derive(Debug, Default)]
pub struct NoTrack(());

/// Attaches a `T`-typed tracker to the result of an `F`-typed `Future`.
#[derive(Debug)]
pub struct TrackFuture<F, T, A>
where
    F: Future,
    A: Track<T, F::Item>,
{
    future: F,
    tracker: Option<T>,
    _p: PhantomData<A>,
}

// ===== impl TrackFuture =====

impl<F, T, A> Future for TrackFuture<F, T, A>
where
    F: Future,
    A: Track<T, F::Item>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut item = try_ready!(self.future.poll());
        if let Some(h) = self.tracker.take() {
            A::track(h, &mut item);
        }
        Ok(item.into())
    }
}

// ===== NoTrack =====

impl<T, V> Track<T, V> for NoTrack {
    fn track(_: T, _: &mut V) {}
}
