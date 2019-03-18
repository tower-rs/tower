extern crate futures;
extern crate tokio_mock_task;
extern crate tower;
extern crate tower_service;
extern crate tower_service_util;

use tower_service::*;
use futures::future::{ok, FutureResult};
use futures::{Async, Poll, Stream};
use std::cell::Cell;
use std::rc::Rc;
use tower::ServiceExt;

type Error = Box<::std::error::Error + Send + Sync>;

#[derive(Debug, Eq, PartialEq)]
struct Srv {
    admit: Rc<Cell<bool>>,
    count: Rc<Cell<usize>>,
}
impl Service<&'static str> for Srv {
    type Response = &'static str;
    type Error = Error;
    type Future = FutureResult<Self::Response, Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if !self.admit.get() {
            return Ok(Async::NotReady);
        }

        self.admit.set(false);
        Ok(Async::Ready(()))
    }

    fn call(&mut self, req: &'static str) -> Self::Future {
        self.count.set(self.count.get() + 1);
        ok(req)
    }
}

macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::Ready(v)) => v,
            Ok(_) => panic!("not ready"),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::NotReady) => {}
            Ok(futures::Async::Ready(v)) => panic!("ready; value = {:?}", v),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
}

#[test]
fn test_in_order() {
    let mut mock = tokio_mock_task::MockTask::new();

    let admit = Rc::new(Cell::new(false));
    let count = Rc::new(Cell::new(0));
    let srv = Srv {
        count: count.clone(),
        admit: admit.clone(),
    };
    let (tx, rx) = futures::unsync::mpsc::unbounded();
    let mut ca = srv.call_all(rx.map_err(|_| "nope"));

    assert_not_ready!(mock.enter(|| ca.poll()));
    tx.unbounded_send("one").unwrap();
    mock.is_notified();
    assert_not_ready!(mock.enter(|| ca.poll()));
    admit.set(true);
    let v = assert_ready!(mock.enter(|| ca.poll()));
    assert_eq!(v, Some("one"));
    assert_not_ready!(mock.enter(|| ca.poll()));
    admit.set(true);
    tx.unbounded_send("two").unwrap();
    mock.is_notified();
    tx.unbounded_send("three").unwrap();
    let v = assert_ready!(mock.enter(|| ca.poll()));
    assert_eq!(v, Some("two"));
    assert_not_ready!(mock.enter(|| ca.poll()));
    admit.set(true);
    let v = assert_ready!(mock.enter(|| ca.poll()));
    assert_eq!(v, Some("three"));
    admit.set(true);
    assert_not_ready!(mock.enter(|| ca.poll()));
    admit.set(true);
    tx.unbounded_send("four").unwrap();
    mock.is_notified();
    let v = assert_ready!(mock.enter(|| ca.poll()));
    assert_eq!(v, Some("four"));
    assert_not_ready!(mock.enter(|| ca.poll()));
    admit.set(true); // need to be ready since impl doesn't know it'll get EOF
    // When we drop the request stream, CallAll should return None.
    drop(tx);
    mock.is_notified();
    let v = assert_ready!(mock.enter(|| ca.poll()));
    assert!(v.is_none());
    assert_eq!(count.get(), 4);

    // We should also be able to recover the wrapped Service.
    assert_eq!(
        ca.into_inner(),
        Srv {
            count: count.clone(),
            admit
        }
    );
}
