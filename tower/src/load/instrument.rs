use futures_core::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Attaches `I`-typed instruments to `V` typed values.
///
/// This utility allows load metrics to have a protocol-agnostic means to track streams
/// past their initial response future. For example, if `V` represents an HTTP response
/// type, an implementation could add `H`-typed handles to each response's extensions to
/// detect when the response is dropped.
///
/// Handles are intended to be RAII guards that primarily implement `Drop` and update load
/// metric state as they are dropped.
///
/// A base `impl<H, V> Instrument<H, V> for NoInstrument` is provided to drop the handle
/// immediately. This is appropriate when a response is discrete and cannot comprise
/// multiple messages.
///
/// In many cases, the `Output` type is simply `V`. However, `Instrument` may alter the
/// type in order to instrument it appropriately. For example, an HTTP Instrument may
/// modify the body type: so an `Instrument` that takes values of type `http::Response<A>`
/// may output values of type `http::Response<B>`.
pub trait Instrument<H, V>: Clone {
    /// The instrumented value type.
    type Output;

    /// Attaches an `H`-typed handle to a `V`-typed value.
    fn instrument(&self, handle: H, value: V) -> Self::Output;
}

/// A `Instrument` implementation that drops each instrument immediately.
#[derive(Clone, Copy, Debug)]
pub struct NoInstrument;

/// Attaches a `I`-typed instruments to the result of an `F`-typed `Future`.
#[pin_project]
#[derive(Debug)]
pub struct InstrumentFuture<F, I, H> {
    #[pin]
    future: F,
    handle: Option<H>,
    instrument: I,
}

// ===== impl InstrumentFuture =====

impl<F, I, H> InstrumentFuture<F, I, H> {
    /// Wraps a future, instrumenting its value if successful.
    pub fn new(instrument: I, handle: H, future: F) -> Self {
        InstrumentFuture {
            future,
            instrument,
            handle: Some(handle),
        }
    }
}

impl<F, I, H, T, E> Future for InstrumentFuture<F, I, H>
where
    F: Future<Output = Result<T, E>>,
    I: Instrument<H, T>,
{
    type Output = Result<I::Output, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let rsp = ready!(this.future.poll(cx))?;
        let h = this.handle.take().expect("handle");
        Poll::Ready(Ok(this.instrument.instrument(h, rsp)))
    }
}

// ===== NoInstrument =====

impl<H, V> Instrument<H, V> for NoInstrument {
    type Output = V;

    fn instrument(&self, handle: H, value: V) -> V {
        drop(handle);
        value
    }
}
