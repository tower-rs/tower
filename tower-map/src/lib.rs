#![doc(html_root_url = "https://docs.rs/tower-map/0.1.0")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]

//! Tower middleware for mapping one request type to another.

mod layer;

pub use crate::layer::MapLayer;
use futures::Poll;

use tower_service::Service;

/// Configure mapping between two services.
#[derive(Clone, Debug)]
pub struct Map<F, S> {
    func: F,
    service: S,
}

impl<F, S> Map<F, S> {
    /// Map to service `S` via closure `F`.
    pub fn new(func: F, service: S) -> Self {
        Self { func, service }
    }
}

impl<F, S, R1, R2> Service<R1> for Map<F, S>
where
    S: Service<R2>,
    F: Fn(R1) -> R2,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, request: R1) -> Self::Future {
        self.service.call((self.func)(request))
    }
}
