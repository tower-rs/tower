use std::marker::PhantomData;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

pub struct With<S, F> {
    inner: S,
    f: F,
}

impl<S, F> With<S, F> {
    pub fn new(inner: S, f: F) -> Self {
        With { inner, f }
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
    _p: PhantomData<fn(OldRequest, NewRequest)>,
}

impl<F, OldRequest, NewRequest> WithLayer<F, OldRequest, NewRequest>
where
    F: Clone,
{
    pub fn new(f: F) -> Self {
        WithLayer { f, _p: PhantomData }
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
