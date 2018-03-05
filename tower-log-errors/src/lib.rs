//! Tower middleware that logs errors returned by a wrapped service.
//!
//! This is useful if those errors would otherwise be ignored or
//! transformed into another error type that might provide less
//! information, such as by `tower-buffer`.

extern crate futures;
extern crate tower;
#[macro_use]
extern crate log;

use futures::{Future, Poll};
use tower::Service;

use std::error::Error;
use std::sync::Arc;

#[macro_export]
macro_rules! log_errors {
    ($inner:expr) => {
        $crate::LogErrors::new($inner).with_target(module_path!())
    };

}

/// Logs error responses.
#[derive(Clone, Debug)]
pub struct LogErrors<T> {
    inner: T,
    level: log::Level,
    target: Option<Arc<String>>,
}

// ===== impl LogErrors =====

impl<T> LogErrors<T> {

    pub fn new(inner: T) -> Self {
        LogErrors {
            inner,
            level: log::Level::Error,
            target: None,
        }
    }

    pub fn with_level(mut self, level: log::Level) -> Self {
        self.level = level;
        self
    }

    pub fn with_target<I: Into<String>>(mut self, target: I) -> Self {
        self.target = Some(Arc::new(target.into()));
        self
    }

    fn child<U>(&self, inner: U) -> LogErrors<U> {
        LogErrors {
            inner,
            level: self.level,
            target: self.target.as_ref().map(Arc::clone),
        }
    }

    fn log_line<E: Error>(&self, error: E, context: &'static str) {
        match (self.target, error.cause()) {
            (Some(ref target), Some(ref cause)) =>
                log!(target: target, self.level, "{}: {}, cause: {}", context, error, cause),
            (Some(ref target), None) =>
                log!(target: target, self.level, "{}: {}", context, error),
            (None, Some(ref cause)) =>
                log!(self.level, "{}: {}, cause: {}", context, error, cause),
            (None, None) =>
                log!(self.level, "{}: {}", context, error),
        }
    }

}

impl<T> Future for LogErrors<T>
where
    T: Future,
    T::Error: Error,
{
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll().map_err(|e| {
            self.log_line(e, "Future::poll");
            e
        })
    }
}

impl<T> Service for LogErrors<T>
where
    T: Service,
    T::Error: Error,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = T::Error;
    type Future = LogErrors<T::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(|e| {
            self.log_line(e, "Service::poll_ready");
            e
        })
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        let inner = self.inner.call(req);
        self.child(inner)
    }
}

impl<T> NewService for LogErrors<T>
where
    T: NewService,
    T::Error: Error,
    T::InitError: Error,
{

    type Request = T::Request;
    type Response = T::Response;
    type Error = T::Error;
    type Service = LogErrors<T::Service>;
    type InitError = T::InitError;
    type Future = LogErrors<T::Future>;

    fn new_service(&self) -> Self::Future {
        self.child(self.inner.new_service())
    }
}
