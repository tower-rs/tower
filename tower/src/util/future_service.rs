use std::fmt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Returns a new `FutureService` for the given future.
///
/// # Example
/// ```
/// use tower::{service_fn, Service, ServiceExt};
/// use tower::util::future_service;
/// use std::convert::Infallible;
///
/// # fn main() {
/// # async {
/// let mut future_of_a_service = future_service(Box::pin(async {
///     let svc = service_fn(|_req: ()| async { Ok::<_, Infallible>("ok") });
///     Ok::<_, Infallible>(svc)
/// }));
///
/// let svc = future_of_a_service.ready_and().await.unwrap();
/// let res = svc.call(()).await.unwrap();
/// # };
/// # }
/// ```
///
/// # Regarding the `Unpin` bound
///
/// The `Unpin` bound on `F` is necessary because the future will be polled in
/// `Service::poll_ready` which doesn't have a pinned receiver (it takes `&mut self` and not `self:
/// Pin<&mut Self>`). So we cannot put the future into a `Pin` without requiring `Unpin`.
///
/// This will most likely come up if you're calling `future_service` with an async block. In that
/// case you can use `Box::pin(async { ... })` as shown in the example.
pub fn future_service<F, S, R, E>(future: F) -> FutureService<F, S>
where
    F: Future<Output = Result<S, E>> + Unpin,
    S: Service<R, Error = E>,
{
    FutureService {
        inner: State::Future(future),
    }
}

/// A type that implements `Service` for a `Future` that produces a `Service`.
#[derive(Clone)]
pub struct FutureService<F, S> {
    inner: State<F, S>,
}

impl<F, S> fmt::Debug for FutureService<F, S>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[derive(Debug)]
        struct Pending;

        let mut f = f.debug_struct("Pending");
        let f = match &self.inner {
            State::Future(_) => f.field("service", &Pending),
            State::Service(svc) => f.field("service", svc),
        };
        f.finish()
    }
}

#[derive(Clone)]
enum State<F, S> {
    Future(F),
    Service(S),
}

impl<F, S, R, E> Service<R> for FutureService<F, S>
where
    F: Future<Output = Result<S, E>> + Unpin,
    S: Service<R, Error = E>,
{
    type Response = S::Response;
    type Error = E;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.inner = match &mut self.inner {
                State::Future(fut) => {
                    let fut = Pin::new(fut);
                    let svc = futures_core::ready!(fut.poll(cx)?);
                    State::Service(svc)
                }
                State::Service(svc) => return svc.poll_ready(cx),
            };
        }
    }

    fn call(&mut self, req: R) -> Self::Future {
        if let State::Service(svc) = &mut self.inner {
            svc.call(req)
        } else {
            panic!("Pending::call was called before Pending::poll_ready")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{future_service, ServiceExt};
    use crate::Service;
    use futures::future::{ready, Ready};
    use std::convert::Infallible;

    #[tokio::test]
    async fn pending_service_debug_impl() {
        let mut pending_svc = future_service(ready(Ok(DebugService)));

        assert_eq!(format!("{:?}", pending_svc), "Pending { service: Pending }");

        pending_svc.ready_and().await.unwrap();

        assert_eq!(
            format!("{:?}", pending_svc),
            "Pending { service: DebugService }"
        );
    }

    #[derive(Debug)]
    struct DebugService;

    impl Service<()> for DebugService {
        type Response = ();
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Ok(()).into()
        }

        fn call(&mut self, _req: ()) -> Self::Future {
            ready(Ok(()))
        }
    }
}
