use std::{
    fmt,
    future::Future,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio_util::sync::ReusableBoxFuture;
use tower_service::{Call, Service};

/// Returns a new [`ServiceFn`] with the given closure.
///
/// This lets you build a [`Service`] from an async function that returns a [`Result`].
///
/// # Example
///
/// ```
/// use tower::{service_fn, Service, ServiceExt, BoxError};
/// # struct Request;
/// # impl Request {
/// #     fn new() -> Self { Self }
/// # }
/// # struct Response(&'static str);
/// # impl Response {
/// #     fn new(body: &'static str) -> Self {
/// #         Self(body)
/// #     }
/// #     fn into_body(self) -> &'static str { self.0 }
/// # }
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), BoxError> {
/// async fn handle(request: Request) -> Result<Response, BoxError> {
///     let response = Response::new("Hello, World!");
///     Ok(response)
/// }
///
/// let mut service = service_fn(handle);
///
/// let response = service
///     .ready_and()
///     .await?
///     .call(Request::new())
///     .await?;
///
/// assert_eq!("Hello, World!", response.into_body());
/// #
/// # Ok(())
/// # }
/// ```
pub fn service_fn<T>(f: T) -> ServiceFn<T> {
    ServiceFn::from_arc(Arc::new(Mutex::new(f)))
}

/// A [`Service`] implemented by a closure.
///
/// See [`service_fn`] for more details.
pub struct ServiceFn<T> {
    f: Arc<Mutex<T>>,
    lock: ReusableBoxFuture<'static, OwnedMutexGuard<T>>,
    needs_lock: bool,
}

pub struct CallServiceFn<T>(OwnedMutexGuard<T>);

// === impl ServiceFn ===

impl<T> ServiceFn<T> {
    fn from_arc(f: Arc<Mutex<T>>) -> Self {
        Self {
            f,
            lock: ReusableBoxFuture::new(async move {
                unreachable!("when a `ServiceFn` is created, needs_lock must be true")
            }),
            needs_lock: true,
        }
    }
}

impl<T> fmt::Debug for ServiceFn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceFn")
            .field("f", &format_args!("{}", std::any::type_name::<T>()))
            .field("needs_lock", &self.needs_lock)
            .finish()
    }
}

impl<T> Clone for ServiceFn<T> {
    fn clone(&self) -> Self {
        Self::from_arc(self.f.clone())
    }
}

impl<T, F, Request, R, E> Service<Request> for ServiceFn<T>
where
    T: FnMut(Request) -> F + Send + 'static,
    F: Future<Output = Result<R, E>>,
{
    type Call = CallServiceFn<T>;
    type Response = R;
    type Error = E;
    type Future = F;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Call, Self::Error>> {
        if self.needs_lock {
            self.lock.set(self.f.clone().lock_owned());
        }

        self.lock.poll(cx).map(|guard| Ok(CallServiceFn(guard)))
    }
}

// === impl CallServiceFn ===

impl<T, F, Request, R, E> Call<Request> for CallServiceFn<T>
where
    T: FnMut(Request) -> F,
    F: Future<Output = Result<R, E>>,
{
    type Response = R;
    type Error = E;
    type Future = F;

    fn call(mut self, req: Request) -> Self::Future {
        (self.0)(req)
    }
}

impl<T> fmt::Debug for CallServiceFn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallServiceFn")
            .field("f", &format_args!("{}", std::any::type_name::<T>()))
            .finish()
    }
}
