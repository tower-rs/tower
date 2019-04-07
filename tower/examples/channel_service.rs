//! Spawns a task to respond to `Service` requests.
//!
//! The example demonstrates how to implement a service to handle backpressure
//! as well as how to use a service that can reach capacity.
//!
//! A task is used to handle service requests. The requests are dispatched to
//! the task using a channel. The task is implemented such requests can pile up
//! (responses are sent back after a fixed timeout).

#![deny(warnings)]

#[macro_use]
extern crate log;

use env_logger;

use tower::{MakeService, ServiceExt};
use tower_service::Service;

use futures::future::{Executor, FutureResult};
use futures::sync::{mpsc, oneshot};
use futures::{Async, Future, IntoFuture, Poll, Stream};
use futures_cpupool::CpuPool;
use tokio_timer::Timer;

use std::io;
use std::time::Duration;

/// Service that dispatches requests to a side task using a channel.
#[derive(Debug)]
pub struct ChannelService {
    // Send the request and a oneshot Sender to push the response into.
    tx: Sender,
}

type Sender = mpsc::Sender<(String, oneshot::Sender<String>)>;

/// Creates new `ChannelService` services.
#[derive(Debug)]
pub struct NewChannelService {
    // The number of requests to buffer
    buffer: usize,

    // The timer
    timer: Timer,

    // Executor to spawn the task on
    pool: CpuPool,
}

/// Response backed by a oneshot.
#[derive(Debug)]
pub struct ResponseFuture {
    rx: Option<oneshot::Receiver<String>>,
}

/// The service error
#[derive(Debug)]
pub enum Error {
    AtCapacity,
    Failed,
}

impl NewChannelService {
    pub fn new(buffer: usize, pool: CpuPool) -> Self {
        let timer = Timer::default();

        NewChannelService {
            buffer,
            timer,
            pool,
        }
    }
}

impl Service<()> for NewChannelService {
    type Response = ChannelService;
    type Error = io::Error;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _target: ()) -> Self::Future {
        let (tx, rx) = mpsc::channel::<(String, oneshot::Sender<String>)>(self.buffer);
        let timer = self.timer.clone();

        // Create the task that proceses the request
        self.pool
            .execute(rx.for_each(move |(msg, tx)| {
                timer.sleep(Duration::from_millis(500)).then(move |res| {
                    res.unwrap();
                    let _ = tx.send(msg);
                    Ok(())
                })
            }))
            .map(|_| ChannelService { tx })
            .map_err(|_| io::ErrorKind::Other.into())
            .into_future()
    }
}

impl Service<String> for ChannelService {
    type Response = String;
    type Error = Error;
    type Future = ResponseFuture;

    fn poll_ready(&mut self) -> Poll<(), Error> {
        self.tx.poll_ready().map_err(|_| Error::Failed)
    }

    fn call(&mut self, request: String) -> ResponseFuture {
        let (tx, rx) = oneshot::channel();

        match self.tx.try_send((request, tx)) {
            Ok(_) => {}
            Err(_) => {
                return ResponseFuture { rx: None };
            }
        }

        ResponseFuture { rx: Some(rx) }
    }
}

impl Future for ResponseFuture {
    type Item = String;
    type Error = Error;

    fn poll(&mut self) -> Poll<String, Error> {
        match self.rx {
            Some(ref mut rx) => match rx.poll() {
                Ok(Async::Ready(v)) => Ok(v.into()),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(_) => Err(Error::Failed),
            },
            None => Err(Error::AtCapacity),
        }
    }
}

pub fn main() {
    env_logger::init();

    let mut new_service = NewChannelService::new(5, CpuPool::new(1));

    // Get the service
    let mut service = new_service.make_service(()).wait().unwrap();
    let mut responses = vec![];

    for i in 0..10 {
        service = service.ready().wait().unwrap();

        info!("sending request; i={}", i);

        let request = format!("request={}", i);
        responses.push(service.call(request));
    }

    for response in responses {
        println!("response={:?}", response.wait());
    }
}
