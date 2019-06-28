#![feature(async_await)]

use futures::future;
// use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tower_service::Service;
use tower_timeout::Timeout;

struct Svc;

impl Service<()> for Svc {
    type Response = ();
    type Error = Never;
    type Future = future::Ready<Result<(), Never>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, _: ()) -> Self::Future {
        future::ready(Ok(()))
    }
}

#[tokio::test]
async fn work() {
    let svc = Svc;

    let mut svc = Timeout::new(svc, Duration::from_secs(1));

    svc.call(()).await.unwrap();
    svc.call(()).await.unwrap();    
}

use std::fmt;
#[derive(Debug)]
/// An error that can never occur.
pub enum Never {}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
        match *self {}
    }
}

impl std::error::Error for Never {}
