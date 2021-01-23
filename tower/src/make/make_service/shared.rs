use std::convert::Infallible;
use std::task::{Context, Poll};
use tower_service::Service;

/// A [`MakeService`] that produces services by cloning an inner service.
///
/// [`MakeService`]: super::MakeService
#[derive(Debug, Clone, Copy)]
pub struct Shared<S> {
    service: S,
}

impl<S> Shared<S> {
    /// Create a new [`Shared`] from a service.
    pub fn new(service: S) -> Self {
        Self { service }
    }
}

impl<S, T> Service<T> for Shared<S>
where
    S: Clone,
{
    type Response = S;
    type Error = Infallible;
    type Future = SharedFuture<S>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _target: T) -> Self::Future {
        SharedFuture(futures_util::future::ready(Ok(self.service.clone())))
    }
}

opaque_future! {
    /// Response future from [`Shared`] services.
    pub type SharedFuture<S> = futures_util::future::Ready<Result<S, Infallible>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::make::MakeService;
    use crate::service_fn;
    use futures::future::poll_fn;

    async fn echo<R>(req: R) -> Result<R, Infallible> {
        Ok(req)
    }

    #[tokio::test]
    async fn as_make_service() {
        let mut shared = Shared::new(service_fn(echo::<&'static str>));

        poll_fn(|cx| MakeService::<(), _>::poll_ready(&mut shared, cx))
            .await
            .unwrap();
        let mut svc = shared.make_service(()).await.unwrap();

        poll_fn(|cx| svc.poll_ready(cx)).await.unwrap();
        let res = svc.call("foo").await.unwrap();

        assert_eq!(res, "foo");
    }

    #[tokio::test]
    async fn as_make_service_into_service() {
        let shared = Shared::new(service_fn(echo::<&'static str>));
        let mut shared = MakeService::<(), _>::into_service(shared);

        poll_fn(|cx| Service::<()>::poll_ready(&mut shared, cx))
            .await
            .unwrap();
        let mut svc = shared.call(()).await.unwrap();

        poll_fn(|cx| svc.poll_ready(cx)).await.unwrap();
        let res = svc.call("foo").await.unwrap();

        assert_eq!(res, "foo");
    }
}
