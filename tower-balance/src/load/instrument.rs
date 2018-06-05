use futures::{Future, Poll};
use std::marker::PhantomData;

/// Attaches `I`-typed instruments to `V` typed values.
///
/// This utility allows load metrics to have a protocol-agnostic means to track streams
/// past their initial response future. For example, if `V` represents an HTTP response
/// type, an implementaton could add `H`-typed handles to each response's extensions to
/// detect when the response is dropped.
///
/// Handles are intended to be RAII guards that primarily implement `Drop` and update load
/// metric state as they are dropped.
///
/// A base `impl<H, V> Instrument<H, V> for NoInstrument` is provided to drop the handle
/// immediately. This is appropriate when a response is discrete and cannot comprise
/// multiple messages.
pub trait Instrument<H, V> {
    type Output;

    /// Attaches an `H`-typed handle to a `V`-typed value.
    fn instrument(handle: H, value: V) -> Self::Output;
}

/// A `Instrument` implementation that drops each instrument immediately.
#[derive(Debug, Default)]
pub struct NoInstrument(());

/// Attaches a `I`-typed instruments to the result of an `F`-typed `Future`.
#[derive(Debug)]
pub struct InstrumentFuture<F, M, H>
where
    F: Future,
    M: Instrument<H, F::Item>,
{
    future: F,
    handle: Option<H>,
    _p: PhantomData<M>,
}

// ===== impl InstrumentFuture =====

impl<F, M, H> InstrumentFuture<F, M, H>
where
    F: Future,
    M: Instrument<H, F::Item>,
{
    pub fn new(handle: H, future: F) -> Self {
        InstrumentFuture {
            future,
            handle: Some(handle),
            _p: PhantomData
        }
    }
}

impl<F, M, H> Future for InstrumentFuture<F, M, H>
where
    F: Future,
    M: Instrument<H, F::Item>,
{
    type Item = M::Output;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let rsp = try_ready!(self.future.poll());
        let h = self.handle.take().expect("handle");
        Ok(M::instrument(h, rsp).into())
    }
}

// ===== NoInstrument =====

impl<H, V> Instrument<H, V> for NoInstrument {
    type Output = V;

    fn instrument(handle: H, value: V) -> V {
        drop(handle);
        value
    }
}
