use futures::{Future, Async, Poll};
use tower_service::Service;
use std::fmt;
use std::mem;
use std::marker::PhantomData;

/// Drives the response future for a "oneshot" service.
///
/// The `Oneshot` utility handles calling the service's infrastructure functions
/// and cleanly closes the service before returning the response.
pub struct Oneshot<T, Request>
where
    T: Service<Request>,
{
    service: T,
    state: State<T::Future>,
    _p: PhantomData<Request>,
}

enum State<T>
where
    T: Future,
{
    Pending(T),
    Ready(Result<T::Item, T::Error>),
    Consumed,
}

impl<T, Request> Oneshot<T, Request>
where
    T: Service<Request>,
{
    pub fn new(future: T::Future, service: T) -> Oneshot<T, Request> {
        Oneshot {
            service,
            state: State::Pending(future),
            _p: PhantomData,
        }
    }

    pub fn into_service(self) -> T {
        self.service
    }
}

impl<T, Request> Future for Oneshot<T, Request>
where
    T: Service<Request>
{
    type Item = T::Response;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::State::*;

        loop {
            match self.state {
                Pending(ref mut fut) => {
                    let _ = self.service.poll_service()?;

                    let res = match fut.poll() {
                        Ok(Async::Ready(v)) => Ok(v),
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(e) => Err(e),
                    };

                    self.state = Ready(res);
                }
                Ready(_) => {
                    try_ready!(self.service.poll_close());

                    return match mem::replace(&mut self.state, Consumed) {
                        Ready(res) => res.map(Async::Ready),
                        _ => panic!(),
                    };
                }
                _ => panic!(),
            }
        }
    }
}

impl<T, Request> fmt::Debug for Oneshot<T, Request>
where
    T: Service<Request> + fmt::Debug,
    T::Response: fmt::Debug,
    T::Error: fmt::Debug,
    T::Future: fmt::Debug,
    Request: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Oneshot")
            .field("service", &self.service)
            .field("state", &self.state)
            .finish()
    }
}

impl<T> fmt::Debug for State<T>
where
    T: Future + fmt::Debug,
    T::Item: fmt::Debug,
    T::Error: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::Pending(ref v) => {
                fmt.debug_tuple("State::Pending")
                    .field(v)
                    .finish()
            }
            State::Ready(ref v) => {
                fmt.debug_tuple("State::Ready")
                    .field(v)
                    .finish()
            }
            State::Consumed => {
                fmt.debug_tuple("State::Consumed")
                    .finish()
            }
        }
    }
}
