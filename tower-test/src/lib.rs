#![doc(html_root_url = "https://docs.rs/tower-test/0.3.0-alpha.1")]
#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

//! Mock `Service` that can be used in tests.

mod macros;
pub mod mock;

use futures_util::future::{ready, Ready};
use std::task::{Context, Poll};
use tower_service::Service;

/// A type for use in doctests.
pub struct MyService;
impl Service<()> for MyService {
    type Response = ();
    type Error = &'static str;
    type Future = Ready<Result<Self::Response, Self::Error>>;
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, _: ()) -> Self::Future {
        ready(Ok(()))
    }
}
