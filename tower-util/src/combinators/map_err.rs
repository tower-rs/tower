use futures_util::{future::MapErr as MapErrFut, TryFutureExt};
use std::marker::PhantomData;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

#[derive(Debug)]
pub struct MapErr<S, F> {
    inner: S,
    f: F,
}

impl<S, F> MapErr<S, F> {
    pub fn new(inner: S, f: F) -> Self {
        MapErr {
            f,
            inner,
        }
    }
}

impl<S, F, Request, Error> Service<Request> for MapErr<S, F>
where
    S: Service<Request>,
    F: FnOnce(S::Error) -> Error,
    F: Clone,
{
    type Response = S::Response;
    type Error = Error;
    type Future = MapErrFut<S::Future, F>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(self.f.clone())
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map_err(self.f.clone())
    }
}

#[derive(Debug)]
pub struct MapErrLayer<F, Request, Error> {
    f: F,
    request: PhantomData<Request>,
    error: PhantomData<Error>,
}

impl<F, Request, Error> MapErrLayer<F, Request, Error>
where
    F: Clone,
{
    pub fn new(f: F) -> Self {
        MapErrLayer {
            f,
            request: PhantomData::default(),
            error: PhantomData::default(),
        }
    }
}

impl<S, F, Request, Error> Layer<S> for MapErrLayer<F, Request, Error>
where
    S: Service<Request>,
    F: FnOnce(S::Error) -> Error,
    F: Clone,
{
    type Service = MapErr<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapErr {
            f: self.f.clone(),
            inner,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tower_layer::Layer;

    struct MockService;

    impl Service<String> for MockService {
        type Response = String;
        type Error = u8;
        type Future = futures_util::future::Ready<Result<String, u8>>;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: String) -> Self::Future {
            futures_util::future::ready(Ok(request))
        }
    }

    #[test]
    fn api() {
        let s = MockService;
        let layer = MapErrLayer::new(|x| format!("{}", x));
        let mut new_service = layer.layer(s);
        async {
            let new_error: Result<_, String> = new_service.call("test".to_string()).await;
        };
    }
}
