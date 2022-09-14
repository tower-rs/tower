use std::task::{Context, Poll};

use tower_service::Service;

/// TODO
#[derive(Debug)]
pub struct ByRef<'a, S>(&'a mut S);

impl<'a, S> ByRef<'a, S> {
    pub(crate) fn new(service: &'a mut S) -> Self {
        Self(service)
    }
}

impl<'a, S, Request> Service<Request> for ByRef<'a, S> where S: Service<Request> {
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        (*self.0).poll_ready(cx)
    }

    #[inline]
    fn call(&mut self, req: Request) -> Self::Future {
        (*self.0).call(req)
    }
}
