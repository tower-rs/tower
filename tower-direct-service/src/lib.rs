#![deny(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tower/0.1.0")]

//! Definition of the `DirectService` trait to Tower
//!
//! This trait provides the necessary abstractions for defining a request /
//! response service that needs to be driven in order to make progress. It
//! is akin to the `Service` trait in `tower-service`, with the additional
//! requirement that `poll_service` must also be called to make progress on
//! pending requests. The trait is simple but powerul, and is used alongside
//! `Service` as the foundation for the rest of Tower.
//!
//! * [`DirectService`](trait.DirectService.html) is the primary trait and
//!   defines the request / response exchange. See that trait for more details.

extern crate futures;

use futures::{Future, Poll};

/// An asynchronous function from `Request` to a `Response` that requires polling.
///
/// A service that implements this trait acts like a future, and needs to be
/// polled for the futures returned from `call` to make progress. In particular,
/// `poll_service` must be called in a similar manner as `Future::poll`; whenever
/// the task driving the `DirectService` is notified.
pub trait DirectService<Request> {
    /// Responses given by the service.
    type Response;

    /// Errors produced by the service.
    type Error;

    /// The future response value.
    type Future: Future<Item = Self::Response, Error = Self::Error>;

    /// Returns `Ready` when the service is able to process requests.
    ///
    /// If the service is at capacity, then `NotReady` is returned and the task
    /// is notified when the service becomes ready again. This function is
    /// expected to be called while on a task.
    ///
    /// This is a **best effort** implementation. False positives are permitted.
    /// It is permitted for the service to return `Ready` from a `poll_ready`
    /// call and the next invocation of `call` results in an error.
    ///
    /// Implementors should call `poll_service` as necessary to finish in-flight
    /// requests.
    fn poll_ready(&mut self) -> Poll<(), Self::Error>;

    /// Returns `Ready` whenever there is no more work to be done until `call`
    /// is invoked again.
    ///
    /// Note that this method may return `NotReady` even if there are no
    /// outstanding requests, if the service has to perform non-request-driven
    /// operations (e.g., heartbeats).
    fn poll_service(&mut self) -> Poll<(), Self::Error>;

    /// A method to indicate that no more requests will be sent to this service.
    ///
    /// This method is used to indicate that a service will no longer be given
    /// another request by the caller. That is, the `call` method will
    /// be called no longer (nor `poll_service`). This method is intended to
    /// model "graceful shutdown" in various protocols where the intent to shut
    /// down is followed by a little more blocking work.
    ///
    /// Callers of this function should work it it in a similar fashion to
    /// `poll_service`. Once called it may return `NotReady` which indicates
    /// that more external work needs to happen to make progress. The current
    /// task will be scheduled to receive a notification in such an event,
    /// however.
    ///
    /// Note that this function will imply `poll_service`. That is, if a
    /// service has pending request, then it'll be flushed out during a
    /// `poll_close` operation. It is not necessary to have `poll_service`
    /// return `Ready` before a `poll_close` is called. Once a `poll_close`
    /// is called, though, `poll_service` cannot be called.
    ///
    /// # Return value
    ///
    /// This function, like `poll_service`, returns a `Poll`. The value is
    /// `Ready` once the close operation has completed. At that point it should
    /// be safe to drop the service and deallocate associated resources, and all
    /// futures returned from `call` should have resolved.
    ///
    /// If the value returned is `NotReady` then the sink is not yet closed and
    /// work needs to be done to close it. The work has been scheduled and the
    /// current task will receive a notification when it's next ready to call
    /// this method again.
    ///
    /// Finally, this function may also return an error.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` if any operation along the way during
    /// the close operation fails. An error typically is fatal for a service and is
    /// unable to be recovered from, but in specific situations this may not
    /// always be true.
    ///
    /// Note that it's also typically an error to call `call` or `poll_service`
    /// after the `poll_close` function is called. This method will *initiate*
    /// a close, and continuing to send values after that (or attempt to flush)
    /// may result in strange behavior, panics, errors, etc. Once this method is
    /// called, it must be the only method called on this `DirectService`.
    ///
    /// # Panics
    ///
    /// This method may panic or cause panics if:
    ///
    /// * It is called outside the context of a future's task
    /// * It is called and then `call` or `poll_service` is called
    fn poll_close(&mut self) -> Poll<(), Self::Error>;

    /// Process the request and return the response asynchronously.
    ///
    /// This function is expected to be callable off task. As such,
    /// implementations should take care to not call any of the `poll_*`
    /// methods. If the service is at capacity and the request is unable
    /// to be handled, the returned `Future` should resolve to an error.
    ///
    /// Calling `call` without calling `poll_ready` is permitted. The
    /// implementation must be resilient to this fact.
    ///
    /// Note that for the returned future to resolve, this `DirectService`
    /// must be driven through calls to `poll_service` or `poll_close`.
    fn call(&mut self, req: Request) -> Self::Future;
}
