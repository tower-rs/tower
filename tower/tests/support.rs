#![allow(dead_code)]

use std::fmt;
use std::future;
use std::task::{Context, Poll};
use tower::Service;

pub(crate) fn trace_init() -> tracing::subscriber::DefaultGuard {
    let subscriber = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::TRACE)
        .with_thread_names(true)
        .finish();
    tracing::subscriber::set_default(subscriber)
}

#[derive(Clone, Debug)]
pub struct AssertSpanSvc {
    span: tracing::Span,
    polled: bool,
}

pub struct AssertSpanError(String);

impl fmt::Debug for AssertSpanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Display for AssertSpanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for AssertSpanError {}

impl AssertSpanSvc {
    pub fn new(span: tracing::Span) -> Self {
        Self {
            span,
            polled: false,
        }
    }

    fn check(&self, func: &str) -> Result<(), AssertSpanError> {
        let current_span = tracing::Span::current();
        tracing::debug!(?current_span, ?self.span, %func);
        if current_span == self.span {
            return Ok(());
        }

        Err(AssertSpanError(format!(
            "{} called outside expected span\n expected: {:?}\n  current: {:?}",
            func, self.span, current_span
        )))
    }
}

impl Service<()> for AssertSpanSvc {
    type Response = ();
    type Error = AssertSpanError;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.polled {
            return Poll::Ready(self.check("poll_ready"));
        }

        cx.waker().wake_by_ref();
        self.polled = true;
        Poll::Pending
    }

    fn call(&mut self, _: ()) -> Self::Future {
        future::ready(self.check("call"))
    }
}
