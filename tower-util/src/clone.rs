//! Contains a `CloneService` type.

use std::sync::{Arc, Mutex};
use futures::{Async, Poll};
use futures::task::{self, Task};
use tower::Service;

/// Be able to clone a `Service`.
#[derive(Debug)]
pub struct CloneService<T> {
    inner: Arc<Mutex<Inner<T>>>,
    send_task: Arc<Mutex<Option<Task>>>,
}

#[derive(Debug)]
struct Inner<T> {
    service: T,
    tasks: Vec<Arc<Mutex<Option<Task>>>>,
}

impl<T> CloneService<T> {
    /// Wrap a service in a cloneable `CloneService`.
    pub fn new(inner: T) -> Self {
        CloneService {
            inner: Arc::new(Mutex::new(Inner {
                service: inner,
                tasks: Vec::new(),
            })),
            send_task: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T> Clone for CloneService<T> {
    fn clone(&self) -> Self {
        CloneService {
            inner: self.inner.clone(),
            send_task: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T> Service for CloneService<T>
where T: Service,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = T::Error;
    type Future = T::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let mut inner = self.inner.lock().unwrap();
        let async = inner.service.poll_ready()?;
        if async.is_ready() {
            for task in inner.tasks.drain(..) {
                task.lock()
                    .unwrap()
                    .take()
                    .map(|t| t.notify());
            }
            Ok(Async::Ready(()))
        } else {
            let should_push = {
                let mut task = self.send_task.lock().unwrap();
                if task.is_none() {
                    *task = Some(task::current());
                    true
                } else {
                    false
                }
            };
            if should_push {
                inner.tasks.push(self.send_task.clone());
            }
            Ok(Async::NotReady)
        }
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        self.inner
            .lock()
            .unwrap()
            .service
            .call(req)
    }
}
