use std::future::Future;
use std::task::{Context, Poll};
use tower_service::Service;

/// Returns a new [`ServiceFn`] with the given closure.
pub fn service_fn<T>(f: T) -> ServiceFn<T> {
    ServiceFn { f }
}

/// A [`Service`] implemented by a closure.
#[derive(Copy, Clone, Debug)]
pub struct ServiceFn<T> {
    f: T,
}

impl<T, F, Request, R, E> Service<Request> for ServiceFn<T>
where
    T: FnMut(Request) -> F,
    F: Future<Output = Result<R, E>>,
{
    type Response = R;
    type Error = E;
    type Future = F;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), E>> {
        Ok(()).into()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        (self.f)(req)
    }
}

impl<T> ServiceFn<T> {
    pub fn poll_ready_fn<F>(self, f: F) -> PollReadyFn<Self, F>
    where
        Self: Sized,
    {
        PollReadyFn { svc: self, f }
    }
}

pub struct PollReadyFn<S, F> {
    svc: S,
    f: F,
}

impl<R, S, F> Service<R> for PollReadyFn<S, F>
where
    S: Service<R>,
    F: FnMut(&mut Context<'_>) -> Poll<Result<(), S::Error>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        (self.f)(cx)
    }

    fn call(&mut self, req: R) -> Self::Future {
        self.svc.call(req)
    }
}
