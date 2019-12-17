use std::task::{Context, Poll};
use std::marker::PhantomData;
use tower_service::Service;
use tower_layer::Layer;

pub struct With<S, F> {
    inner: S,
    f: F,
}

impl<S, F> With<S, F> {
    pub fn new(inner: S, f: F) -> Self {
        With {
            inner,
            f
        }
    }
}

impl<S, F, NewRequest, OldRequest> Service<NewRequest> for With<S, F>
where
    S: Service<OldRequest>,
    F: FnOnce(NewRequest) -> OldRequest,
    F: Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: NewRequest) -> S::Future {
        self.inner.call((self.f.clone())(request))
    }
}

#[derive(Debug)]
pub struct WithLayer<F, OldRequest, NewRequest> {
    f: F,
    old_request: PhantomData<OldRequest>,
    new_request: PhantomData<NewRequest>
}

impl<F, OldRequest, NewRequest> WithLayer<F, OldRequest, NewRequest>
where
    F: Clone,
{
    pub fn new(f: F) -> Self {
        WithLayer {
            f,
            old_request: PhantomData::default(),
            new_request: PhantomData::default()
        }
    }
}

impl<S, F, OldRequest, NewRequest> Layer<S> for WithLayer<F, OldRequest, NewRequest>
where
    S: Service<OldRequest>,
    F: FnOnce(NewRequest) -> OldRequest,
    F: Clone,
{
    type Service = With<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        With {
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
        let layer = WithLayer::new(|x| {
            format!("{}", x)
        });
        let mut new_service = layer.layer(s);

        let new_request: u32 = 3;
        new_service.call(3);
    }
}
