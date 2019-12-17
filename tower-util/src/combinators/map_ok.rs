use futures_util::{future::MapOk as MapOkFut, TryFutureExt};
use std::marker::PhantomData;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

#[derive(Debug)]
pub struct MapOk<S, F> {
    inner: S,
    f: F,
}

impl<S, F> MapOk<S, F> {
    pub fn new(inner: S, f: F) -> Self {
        MapOk {
            f,
            inner,
        }
    }
}

impl<S, F, Request, Response> Service<Request> for MapOk<S, F>
where
    S: Service<Request>,
    F: FnOnce(S::Response) -> Response,
    F: Clone,
{
    type Response = Response;
    type Error = S::Error;
    type Future = MapOkFut<S::Future, F>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map_ok(self.f.clone())
    }
}

#[derive(Debug)]
pub struct MapOkLayer<F, Request, Response> {
    f: F,
    request: PhantomData<Request>,
    response: PhantomData<Response>,
}

impl<F, Request, Error> MapOkLayer<F, Request, Error>
where
    F: Clone,
{
    pub fn new(f: F) -> Self {
        MapOkLayer {
            f,
            request: PhantomData::default(),
            response: PhantomData::default(),
        }
    }
}

impl<S, F, Request, Response> Layer<S> for MapOkLayer<F, Request, Response>
where
    S: Service<Request>,
    F: FnOnce(S::Response) -> Response,
    F: Clone,
{
    type Service = MapOk<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapOk {
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
        let layer = MapOkLayer::new(|x: String| {
            x.as_bytes().to_vec()
        });
        let mut new_service = layer.layer(s);
        async {
            let new_response: Result<Vec<u8>, _> = new_service.call("test".to_string()).await;
        };
    }
}
