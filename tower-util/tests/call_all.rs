use futures::{
    self,
    future::{ok, FutureResult},
    stream, Async, Poll, Stream,
};
use std::{cell::Cell, rc::Rc};
use tokio_mock_task;
use tower::ServiceExt;
use tower_mock::*;
use tower_service::*;

type Error = Box<dyn std::error::Error + Send + Sync>;

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
fn ordered() {
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

    // need to be ready since impl doesn't know it'll get EOF
    admit.set(true);

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

#[test]
fn unordered() {
    let (mock, mut handle) = Mock::<_, &'static str>::new();
    let mut task = tokio_mock_task::MockTask::new();
    let requests = stream::iter_ok::<_, Error>(&["one", "two"]);

    let mut svc = mock.call_all(requests).unordered();
    assert_not_ready!(task.enter(|| svc.poll()));

    let (req1, resp1) = handle.next_request().unwrap().into_parts();
    let (req2, resp2) = handle.next_request().unwrap().into_parts();

    assert_eq!(req1, &"one");
    assert_eq!(req2, &"two");

    resp2.respond("resp 1");

    let v = assert_ready!(task.enter(|| svc.poll()));
    assert_eq!(v, Some("resp 1"));
    assert_not_ready!(task.enter(|| svc.poll()));

    resp1.respond("resp 2");

    let v = assert_ready!(task.enter(|| svc.poll()));
    assert_eq!(v, Some("resp 2"));

    let v = assert_ready!(task.enter(|| svc.poll()));
    assert!(v.is_none());
}
