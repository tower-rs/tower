//! Various utility types and functions that are generally with Tower.

mod boxed;
mod call_all;
mod either;
mod map_err;
mod map_request;
mod map_response;
mod oneshot;
mod optional;
mod ready;
mod service_fn;
mod try_map_request;

pub use self::{
    boxed::{BoxService, UnsyncBoxService},
    either::Either,
    map_err::{MapErr, MapErrLayer},
    map_request::{MapRequest, MapRequestLayer},
    map_response::{MapResponse, MapResponseLayer},
    oneshot::Oneshot,
    optional::Optional,
    ready::{Ready, ReadyAnd, ReadyOneshot},
    service_fn::{service_fn, ServiceFn},
    try_map_request::{TryMapRequest, TryMapRequestLayer},
};

pub use self::call_all::{CallAll, CallAllUnordered};

pub mod error {
    //! Error types

    pub use super::optional::error as optional;
}

pub mod future {
    //! Future types

    pub use super::map_err::MapErrFuture;
    pub use super::map_response::MapResponseFuture;
    pub use super::optional::future as optional;
}

/// An extension trait for `Service`s that provides a variety of convenient
/// adapters
pub trait ServiceExt<Request>: tower_service::Service<Request> {
    /// Resolves when the service is ready to accept a request.
    #[deprecated(since = "0.3.1", note = "prefer `ready_and` which yields the service")]
    fn ready(&mut self) -> Ready<'_, Self, Request>
    where
        Self: Sized,
    {
        #[allow(deprecated)]
        Ready::new(self)
    }

    /// Yields a mutable reference to the service when it is ready to accept a request.
    fn ready_and(&mut self) -> ReadyAnd<'_, Self, Request>
    where
        Self: Sized,
    {
        ReadyAnd::new(self)
    }

    /// Yields the service when it is ready to accept a request.
    fn ready_oneshot(self) -> ReadyOneshot<Self, Request>
    where
        Self: Sized,
    {
        ReadyOneshot::new(self)
    }

    /// Consume this `Service`, calling with the providing request once it is ready.
    fn oneshot(self, req: Request) -> Oneshot<Self, Request>
    where
        Self: Sized,
    {
        Oneshot::new(self, req)
    }

    /// Process all requests from the given `Stream`, and produce a `Stream` of their responses.
    ///
    /// This is essentially `Stream<Item = Request>` + `Self` => `Stream<Item = Response>`. See the
    /// documentation for [`CallAll`](struct.CallAll.html) for details.
    fn call_all<S>(self, reqs: S) -> CallAll<Self, S>
    where
        Self: Sized,
        Self::Error: Into<crate::BoxError>,
        S: futures_core::Stream<Item = Request>,
    {
        CallAll::new(self, reqs)
    }

    /// Maps this service's response value to a different value. This does not
    /// alter the behaviour of the [`poll_ready`] method.
    ///
    /// This method can be used to change the [`Response`] type of the service
    /// into a different type. It is similar to the [`Result::map`]
    /// method. You can use this method to chain along a computation once the
    /// services response has been resolved.
    ///
    /// [`Response`]: crate::Service::Response
    /// [`poll_ready`]: crate::Service::poll_ready
    ///
    /// # Example
    /// ```
    /// # use std::task::{Poll, Context};
    /// # use tower::{Service, ServiceExt};
    /// #
    /// # struct DatabaseService;
    /// # impl DatabaseService {
    /// #   fn new(address: &str) -> Self {
    /// #       DatabaseService
    /// #   }
    /// # }
    /// #
    /// # struct Record {
    /// #   pub name: String,
    /// #   pub age: u16
    /// # }
    /// #
    /// # impl Service<u32> for DatabaseService {
    /// #   type Response = Record;
    /// #   type Error = u8;
    /// #   type Future = futures_util::future::Ready<Result<Record, u8>>;
    /// #
    /// #   fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    /// #       Poll::Ready(Ok(()))
    /// #   }
    /// #
    /// #   fn call(&mut self, request: u32) -> Self::Future {
    /// #       futures_util::future::ready(Ok(Record { name: "Jack".into(), age: 32 }))
    /// #   }
    /// # }
    /// #
    /// # fn main() {
    /// #    async {
    /// // A service returning Result<Record, _>
    /// let service = DatabaseService::new("127.0.0.1:8080");
    ///
    /// // Map the response into a new response
    /// let mut new_service = service.map_response(|record| record.name);
    ///
    /// // Call the new service
    /// let id = 13;
    /// let name = new_service.call(id).await.unwrap();
    /// #    };
    /// # }
    /// ```
    fn map_response<F, Response>(self, f: F) -> MapResponse<Self, F>
    where
        Self: Sized,
        F: FnOnce(Self::Response) -> Response + Clone,
    {
        MapResponse::new(self, f)
    }

    /// Maps this services's error value to a different value. This does not
    /// alter the behaviour of the [`poll_ready`] method.
    ///
    /// This method can be used to change the [`Error`] type of the service
    /// into a different type. It is similar to the [`Result::map_err`] method.
    ///
    /// [`Error`]: crate::Service::Error
    /// [`poll_ready`]: crate::Service::poll_ready
    ///
    /// # Example
    /// ```
    /// # use std::task::{Poll, Context};
    /// # use tower::{Service, ServiceExt};
    /// #
    /// # struct DatabaseService;
    /// # impl DatabaseService {
    /// #   fn new(address: &str) -> Self {
    /// #       DatabaseService
    /// #   }
    /// # }
    /// #
    /// # struct Error {
    /// #   pub code: u32,
    /// #   pub message: String
    /// # }
    /// #
    /// # impl Service<u32> for DatabaseService {
    /// #   type Response = String;
    /// #   type Error = Error;
    /// #   type Future = futures_util::future::Ready<Result<String, Error>>;
    /// #
    /// #   fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    /// #       Poll::Ready(Ok(()))
    /// #   }
    /// #
    /// #   fn call(&mut self, request: u32) -> Self::Future {
    /// #       futures_util::future::ready(Ok(String::new()))
    /// #   }
    /// # }
    /// #
    /// # fn main() {
    /// #   async {
    /// // A service returning Result<_, Error>
    /// let service = DatabaseService::new("127.0.0.1:8080");
    ///
    /// // Map the error to a new error
    /// let mut new_service = service.map_err(|err| err.code);
    ///
    /// // Call the new service
    /// let id = 13;
    /// let code = new_service.call(id).await.unwrap_err();
    /// #   };
    /// # }
    /// ```
    fn map_err<F, Error>(self, f: F) -> MapErr<Self, F>
    where
        Self: Sized,
        F: FnOnce(Self::Error) -> Error + Clone,
    {
        MapErr::new(self, f)
    }

    /// Composes a function *in front of* the service.
    ///
    /// This adapter produces a new service that passes each value through the
    /// given function `f` before sending it to `self`.
    ///
    /// # Example
    /// ```
    /// # use std::convert::TryFrom;
    /// # use std::task::{Poll, Context};
    /// # use tower::{Service, ServiceExt};
    /// #
    /// # struct DatabaseService;
    /// # impl DatabaseService {
    /// #   fn new(address: &str) -> Self {
    /// #       DatabaseService
    /// #   }
    /// # }
    /// #
    /// # impl Service<String> for DatabaseService {
    /// #   type Response = String;
    /// #   type Error = u8;
    /// #   type Future = futures_util::future::Ready<Result<String, u8>>;
    /// #
    /// #   fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    /// #       Poll::Ready(Ok(()))
    /// #   }
    /// #
    /// #   fn call(&mut self, request: String) -> Self::Future {
    /// #       futures_util::future::ready(Ok(String::new()))
    /// #   }
    /// # }
    /// #
    /// # fn main() {
    /// #   async {
    /// // A service taking a String as a request
    /// let service = DatabaseService::new("127.0.0.1:8080");
    ///
    /// // Map the request to a new request
    /// let mut new_service = service.map_request(|id: u32| id.to_string());
    ///
    /// // Call the new service
    /// let id = 13;
    /// let response = new_service.call(id).await;
    /// #    };
    /// # }
    /// ```
    fn map_request<F, NewRequest>(self, f: F) -> MapRequest<Self, F>
    where
        Self: Sized,
        F: FnMut(NewRequest) -> Request + Clone,
    {
        MapRequest::new(self, f)
    }

    /// Composes a fallible function *in front of* the service.
    ///
    /// This adapter produces a new service that passes each value through the
    /// given function `f` before sending it to `self`.
    ///
    /// # Example
    /// ```
    /// # use std::convert::TryFrom;
    /// # use std::task::{Poll, Context};
    /// # use tower::{Service, ServiceExt};
    /// #
    /// # struct DatabaseService;
    /// # impl DatabaseService {
    /// #   fn new(address: &str) -> Self {
    /// #       DatabaseService
    /// #   }
    /// # }
    /// #
    /// # enum DbError {
    /// #   Parse(std::num::ParseIntError)
    /// # }
    /// #
    /// # impl Service<u32> for DatabaseService {
    /// #   type Response = String;
    /// #   type Error = DbError;
    /// #   type Future = futures_util::future::Ready<Result<String, DbError>>;
    /// #
    /// #   fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    /// #       Poll::Ready(Ok(()))
    /// #   }
    /// #
    /// #   fn call(&mut self, request: u32) -> Self::Future {
    /// #       futures_util::future::ready(Ok(String::new()))
    /// #   }
    /// # }
    /// #
    /// # fn main() {
    /// #    async {
    /// // A service taking a u32 as a request and returning Result<_, DbError>
    /// let service = DatabaseService::new("127.0.0.1:8080");
    ///
    /// // Fallibly map the request to a new request
    /// let mut new_service = service
    ///     .try_map_request(|id_str: &str| id_str.parse().map_err(DbError::Parse));
    ///
    /// // Call the new service
    /// let id = "13";
    /// let response = new_service.call(id).await;
    /// #    };
    /// # }
    /// ```
    fn try_map_request<F, NewRequest>(self, f: F) -> TryMapRequest<Self, F>
    where
        Self: Sized,
        F: FnMut(NewRequest) -> Result<Request, Self::Error>,
    {
        TryMapRequest::new(self, f)
    }
}

impl<T: ?Sized, Request> ServiceExt<Request> for T where T: tower_service::Service<Request> {}
