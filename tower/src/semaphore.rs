pub(crate) use self::sync::OwnedSemaphorePermit as Permit;
use futures_core::ready;
use std::{
    fmt,
    future::Future,
    mem,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync;

#[derive(Debug)]
pub(crate) struct Semaphore {
    semaphore: Arc<sync::Semaphore>,
    state: State,
}

enum State {
    Waiting(Pin<Box<dyn Future<Output = Permit> + Send + 'static>>),
    Ready(Permit),
    Empty,
}

impl Semaphore {
    pub(crate) fn new(permits: usize) -> Self {
        Self {
            semaphore: Arc::new(sync::Semaphore::new(permits)),
            state: State::Empty,
        }
    }

    pub(crate) fn poll_acquire(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        loop {
            self.state = match self.state {
                State::Ready(_) => return Poll::Ready(()),
                State::Waiting(ref mut fut) => {
                    let permit = ready!(Pin::new(fut).poll(cx));
                    State::Ready(permit)
                }
                State::Empty => State::Waiting(Box::pin(self.semaphore.clone().acquire_owned())),
            };
        }
    }

    pub(crate) fn take_permit(&mut self) -> Option<Permit> {
        if let State::Ready(permit) = mem::replace(&mut self.state, State::Empty) {
            return Some(permit);
        }
        None
    }
}

impl Clone for Semaphore {
    fn clone(&self) -> Self {
        Self {
            semaphore: self.semaphore.clone(),
            state: State::Empty,
        }
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Waiting(_) => f
                .debug_tuple("State::Waiting")
                .field(&format_args!("..."))
                .finish(),
            State::Ready(ref r) => f.debug_tuple("State::Ready").field(&r).finish(),
            State::Empty => f.debug_tuple("State::Empty").finish(),
        }
    }
}
