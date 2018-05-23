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
    type Tracked;

    /// Attaches a `T`-typed trakcer to a `V`-typed value.
    fn track(tracker: T, value: V) -> Self::Tracked;

    /// Wraps an `F`-typred Future so that a `T`-typed tracker is attached to its result.
    fn track_future<F>(tracker: T, future: F) -> TrackFuture<F, Self, T>
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

/// A `Track` implementation that drops each tracker immediately.
#[derive(Debug, Default)]
pub struct NoTrack(());

/// Attaches a `T`-typed tracker to the result of an `F`-typed `Future`.
#[derive(Debug)]
pub struct TrackFuture<F, T, K>
where
    F: Future,
    T: Track<K, F::Item>,
{
    future: F,
    tracker: Option<K>,
    _p: PhantomData<T>,
}

// ===== impl TrackFuture =====

impl<F, T, K> Future for TrackFuture<F, T, K>
where
    F: Future,
    T: Track<K, F::Item>,
{
    type Item = T::Tracked;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let rsp = try_ready!(self.future.poll());
        let track = self.tracker.take().expect("tracker");
        Ok(T::track(track, rsp).into())
    }
}

// ===== NoTrack =====

impl<T, V> Track<T, V> for NoTrack {
    type Tracked = V;

    fn track(tracker: T, v: V) -> V {
        drop(tracker);
        v
    }
}
