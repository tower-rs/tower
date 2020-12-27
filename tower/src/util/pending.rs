use std::fmt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Returns a new `Pending` for the given future.
///
/// # Example
/// ```
/// use tower::{service_fn, Service, ServiceExt};
/// use tower::util::pending;
/// use std::convert::Infallible;
///
/// # fn main() {
/// # async {
/// let mut pending_svc = pending(Box::pin(async {
///     let svc = service_fn(|_req: ()| async { Ok::<_, Infallible>("ok") });
///     Ok::<_, Infallible>(svc)
/// }));
///
/// let svc = pending_svc.ready_and().await.unwrap();
/// let res = svc.call(()).await.unwrap();
/// # };
/// # }
/// ```
pub fn pending<F, S, R, E>(future: F) -> Pending<F, S>
where
    F: Future<Output = Result<S, E>> + Unpin,
    S: Service<R, Error = E>,
{
    Pending {
        inner: Inner::Future(future),
    }
}

/// A type that implements `Service` for a `Future` that produces a `Service`.
#[derive(Clone)]
pub struct Pending<F, S> {
    inner: Inner<F, S>,
}

impl<F, S> fmt::Debug for Pending<F, S>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[derive(Debug)]
        struct Pending;

        let mut f = f.debug_struct("Pending");
        let f = match &self.inner {
            Inner::Future(_) => f.field("service", &Pending),
            Inner::Service(svc) => f.field("service", svc),
        };
        f.finish()
    }
}

#[derive(Clone)]
enum Inner<F, S> {
    Future(F),
    Service(S),
}

impl<F, S, R, E> Service<R> for Pending<F, S>
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
                Inner::Future(fut) => {
                    let fut = Pin::new(fut);
                    let svc = match fut.poll(cx) {
                        Poll::Ready(Ok(svc)) => svc,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    };
                    Inner::Service(svc)
                }
                Inner::Service(svc) => return svc.poll_ready(cx),
            };
        }
    }

    fn call(&mut self, req: R) -> Self::Future {
        if let Inner::Service(svc) = &mut self.inner {
            svc.call(req)
        } else {
            panic!("Pending::call was called before Pending::poll_ready")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{pending, ServiceExt};
    use crate::Service;
    use futures::future::{ready, Ready};
    use std::convert::Infallible;

    #[tokio::test]
    async fn pending_service_debug_impl() {
        let mut pending_svc = pending(ready(Ok(DebugService)));

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
