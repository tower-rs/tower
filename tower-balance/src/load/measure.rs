use futures::{Future, Poll};
use std::marker::PhantomData;

/// Attaches `I`-typed instruments to `V` typed values.
///
/// This utility allows load metrics to have a protocol-agnostic means to measure streams
/// past their initial response future. For example, if `V` represents an HTTP response
/// type, an implementaton could add `I`-typed instruments to each response's extensions
/// to detect when the response is dropped.
///
/// `Instrument`s are intended to be RAII guards that primarily implement `Drop` and
/// update load metric state as they are dropped.
///
/// A base `impl<I, V> Measure<I, V> for NoMeasure` is provided to drop the instrument
/// immediately. This is appropriate when a response is discrete and cannot comprise
/// multiple messages.
pub trait Measure<I, V> {
    type Measured;

    /// Attaches an `I`-typed instrument to a `V`-typed value.
    fn measure(instrument: I, value: V) -> Self::Measured;
}

/// A `Measure` implementation that drops each instrument immediately.
#[derive(Debug, Default)]
pub struct NoMeasure(());

/// Attaches a `I`-typed instruments to the result of an `F`-typed `Future`.
#[derive(Debug)]
pub struct MeasureFuture<F, M, I>
where
    F: Future,
    M: Measure<I, F::Item>,
{
    future: F,
    instrument: Option<I>,
    _p: PhantomData<M>,
}

// ===== impl MeasureFuture =====

impl<F, M, I> MeasureFuture<F, M, I>
where
    F: Future,
    M: Measure<I, F::Item>,
{
    pub fn new(instrument: I, future: F) -> Self {
        MeasureFuture {
            future,
            instrument: Some(instrument),
            _p: PhantomData
        }
    }
}

impl<F, M, I> Future for MeasureFuture<F, M, I>
where
    F: Future,
    M: Measure<I, F::Item>,
{
    type Item = M::Measured;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let rsp = try_ready!(self.future.poll());
        let ins = self.instrument.take().expect("instrument");
        Ok(M::measure(ins, rsp).into())
    }
}

// ===== NoMeasure =====

impl<I, V> Measure<I, V> for NoMeasure {
    type Measured = V;

    fn measure(instrument: I, value: V) -> V {
        drop(instrument);
        value
    }
}
