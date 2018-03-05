//! Tower middleware that logs errors returned by a wrapped service.
//!
//! This is useful if those errors would otherwise be ignored or
//! transformed into another error type that might provide less
//! information, such as by `tower-buffer`.

extern crate futures;
extern crate tower;
extern crate log;

use futures::{Future, Poll};
use tower::{Service, NewService};

use std::error::Error;

/// Wrap a `Service` or `NewService` with `LogErrors` middleware.
///
/// Unlike using `LogErrors::new`, this macro will configure the returned
/// middleware to log messages with the module path and file/line location of
/// the _call site_, as using the `log!` macro in that file would.
///
/// For example:
/// ```rust,ignore
/// mod my_module {
///     fn my_function<S>(service: S) -> LogErrors<S>
///     where S: Service,
///           S::Error: Error,
///     {
///         let timeout = Timeout::new(
///             service, timer, Duration::from_secs(1)
///         );
///         log_errors!(timeout)
///     }
/// }
/// ```
/// will produce log messages like
/// ```notrust,ignore
/// ERROR 2018-03-05T18:36:36Z: my_crate::my_module: Future::poll: operation timed out after Duration { secs: 1, nanos: 0 }
/// ```
/// while
/// ```rust,ignore
/// mod my_module {
///     fn my_function<S>(service: S) -> LogErrors<S>
///     where S: Service,
///           S::Error: Error,
///     {
///         let timeout = Timeout::new(
///             service, timer, Duration::from_secs(1)
///         );
///         LogErrors::new(timeout)
///     }
/// }
/// ```
/// will produce log messages like
/// ```notrust,ignore
/// ERROR 2018-03-05T18:36:36Z: tower_log_errors: Future::poll: operation timed out after Duration { secs: 1, nanos: 0 }
/// ```
#[macro_export]
macro_rules! log_errors {
    ($inner:expr) => {
        log_errors!(level: ::log::Level::Error, $inner)
    };
    (target: $target:expr, $($rest:tt)+) => {
        log_errors!($($rest),+).target($target)
    };
    (level: $level:expr, $inner:expr) => {
        $crate::LogErrors::new($inner)
            .in_module(module_path!())
            .at_location(file!(), line!())
            .at_level($level)
    };

}

/// Logs error responses.
#[derive(Clone, Debug)]
pub struct LogErrors<T> {
    inner: T,
    level: log::Level,
    target: Option<&'static str>,
    module_path: Option<&'static str>,
    file: Option<&'static str>,
    line: Option<u32>,
}

// ===== impl LogErrors =====

impl<T> LogErrors<T> {

    /// Construct a new `LogErrors` middleware that wraps the given `Service`
    /// or `NewService`.
    ///
    /// The log level will default to `Level::Error` but may be changed with
    /// the [`at_level`] function.
    ///
    /// # Note
    ///
    /// Unless the module path of the `LogErrors` middleware is changed with
    /// the [`in_module`]  methods, log records produced by the returned
    /// middleware will always have the module path `tower-log-errors`. It may
    /// be preferred to use the [`log_errors!`] macro instead, as it will
    /// produce log records which appear to have been produced at the call site.
    ///
    /// [`at_level`]: struct.LogErrors.html#method.at_level
    /// [`in_module`]: struct.LogErrors.html#method.in_module
    /// [`log_errors!`]: macro.log_errors.html
    pub fn new(inner: T) -> Self {
        LogErrors {
            inner,
            level: log::Level::Error,
            target: None,
            module_path: None,
            file: None,
            line: None,
        }
    }

    /// Set the log level of the produced log records.
    ///
    /// Log records will be logged at the `Error` level by default.
    pub fn at_level(mut self, level: log::Level) -> Self {
        self.level = level;
        self
    }

    /// Set the target of the produced log records.
    ///
    /// The target will default to the module path of the `LogErrors` middleware
    /// by default.
    pub fn with_target(mut self, target: &'static str) -> Self {
        self.target = Some(target);
        self
    }

    /// Set the module path of the produced log records to the given string.
    pub fn in_module(mut self, module_path: &'static str) -> Self {
        self.module_path = Some(module_path);
        self
    }

    /// Set the file and line number of the produced log records.
    pub fn at_location(mut self, file: &'static str, line: u32) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self
    }

    fn child<U>(&self, inner: U) -> LogErrors<U> {
        LogErrors {
            inner,
            level: self.level,
            target: self.target,
            module_path: self.module_path,
            file: self.file,
            line: self.line,
        }
    }

    fn log_line<E: Error>(&self, error: &E, context: &'static str) {
        log::Log::log(
            log::logger(),
            &log::RecordBuilder::new()
                .level(self.level)
                .file(self.file.or_else(|| Some(file!())))
                .line(self.line.or_else(|| Some(line!())))
                .target(
                    self.target
                        .or(self.module_path)
                        .unwrap_or_else(|| module_path!())
                )
                .module_path(
                    self.module_path
                        .or(self.target)
                        .or_else(|| Some(module_path!())))
                .args(format_args!("{}: {}", context, error))
                .build()
        )

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
            self.log_line(&e, "Future::poll");
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
            self.log_line(&e, "Service::poll_ready");
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
    type Service = T::Service;
    type InitError = T::InitError;
    type Future = LogErrors<T::Future>;

    fn new_service(&self) -> Self::Future {
        self.child(self.inner.new_service())
    }
}
