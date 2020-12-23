#![allow(dead_code)]

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio_stream::Stream;

pub(crate) fn trace_init() -> tracing::subscriber::DefaultGuard {
    let subscriber = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::TRACE)
        .with_thread_names(true)
        .finish();
    tracing::subscriber::set_default(subscriber)
}

#[pin_project::pin_project]
#[derive(Clone, Debug)]
pub struct IntoStream<S>(#[pin] pub S);

impl<I> Stream for IntoStream<mpsc::Receiver<I>> {
    type Item = I;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().0.poll_recv(cx)
    }
}

impl<I> Stream for IntoStream<mpsc::UnboundedReceiver<I>> {
    type Item = I;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().0.poll_recv(cx)
    }
}
