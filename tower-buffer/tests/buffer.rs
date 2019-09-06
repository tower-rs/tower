use futures_util::pin_mut;
use std::future::Future;
use std::{cell::RefCell, thread};
use tokio_executor::{SpawnError, TypedExecutor};
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok, task};
use tower_buffer::{error, Buffer};
use tower_service::Service;
use tower_test::{assert_request_eq, mock};

#[test]
fn req_and_res() {
    task::mock(|cx| {
        let (mut service, handle) = new_service();

        let response = service.call("hello");
        pin_mut!(response);
        pin_mut!(handle);

        assert_request_eq!(handle, "hello").send_response("world");
        assert_eq!(assert_ready_ok!(response.as_mut().poll(cx)), "world");
    });
}

#[test]
fn clears_canceled_requests() {
    task::mock(|cx| {
        let (mut service, handle) = new_service();
        pin_mut!(handle);

        handle.allow(1);

        let res1 = service.call("hello");
        pin_mut!(res1);

        let send_response1 = assert_request_eq!(handle.as_mut(), "hello");

        // don't respond yet, new requests will get buffered

        let res2 = service.call("hello2");

        assert_pending!(handle.as_mut().poll_request(cx));

        let res3 = service.call("hello3");
        pin_mut!(res3);

        drop(res2);

        send_response1.send_response("world");
        assert_eq!(assert_ready_ok!(res1.poll(cx)), "world");

        // res2 was dropped, so it should have been canceled in the buffer
        handle.allow(1);

        assert_request_eq!(handle, "hello3").send_response("world3");

        assert_eq!(assert_ready_ok!(res3.poll(cx)), "world3");
    });
}

#[test]
fn when_inner_is_not_ready() {
    task::mock(|cx| {
        let (mut service, handle) = new_service();
        pin_mut!(handle);

        // Make the service NotReady
        handle.allow(0);

        let res1 = service.call("hello");
        pin_mut!(res1);

        // Allow the Buffer's executor to do work
        ::std::thread::sleep(::std::time::Duration::from_millis(100));
        assert_pending!(res1.as_mut().poll(cx));
        assert_pending!(handle.as_mut().poll_request(cx));

        handle.allow(1);

        assert_request_eq!(handle, "hello").send_response("world");

        assert_eq!(assert_ready_ok!(res1.poll(cx)), "world");
    });
}

#[test]
fn when_inner_fails() {
    task::mock(|cx| {
        use std::error::Error as StdError;

        let (mut service, mut handle) = new_service();

        // Make the service NotReady
        handle.allow(0);
        handle.send_error("foobar");

        let res1 = service.call("hello");
        pin_mut!(res1);

        // Allow the Buffer's executor to do work
        ::std::thread::sleep(::std::time::Duration::from_millis(100));
        let e = assert_ready_err!(res1.poll(cx));
        if let Some(e) = e.downcast_ref::<error::ServiceError>() {
            let e = e.source().unwrap();

            assert_eq!(e.to_string(), "foobar");
        } else {
            panic!("unexpected error type: {:?}", e);
        }
    });
}

#[test]
fn when_spawn_fails() {
    task::mock(|cx| {
        let (service, _handle) = mock::pair::<(), ()>();

        let mut exec = ExecFn(|_| Err(()));

        let mut service = Buffer::with_executor(service, 1, &mut exec);

        let err = assert_ready_err!(service.poll_ready(cx));

        assert!(
            err.is::<error::SpawnError>(),
            "should be a SpawnError: {:?}",
            err
        );
    })
}

#[test]
fn poll_ready_when_worker_is_dropped_early() {
    task::mock(|cx| {
        let (service, _handle) = mock::pair::<(), ()>();

        // drop that worker right on the floor!
        let mut exec = ExecFn(|fut| {
            drop(fut);
            Ok(())
        });

        let mut service = Buffer::with_executor(service, 1, &mut exec);

        let err = assert_ready_err!(service.poll_ready(cx));

        assert!(err.is::<error::Closed>(), "should be a Closed: {:?}", err);
    });
}

#[test]
fn response_future_when_worker_is_dropped_early() {
    task::mock(|cx| {
        let (service, mut handle) = mock::pair::<_, ()>();

        // hold the worker in a cell until we want to drop it later
        let cell = RefCell::new(None);
        let mut exec = ExecFn(|fut| {
            *cell.borrow_mut() = Some(fut);
            Ok(())
        });

        let mut service = Buffer::with_executor(service, 1, &mut exec);

        // keep the request in the worker
        handle.allow(0);
        let response = service.call("hello");
        pin_mut!(response);

        // drop the worker (like an executor closing up)
        cell.borrow_mut().take();

        let err = assert_ready_err!(response.poll(cx));
        assert!(err.is::<error::Closed>(), "should be a Closed: {:?}", err);
    })
}

type Mock = mock::Mock<&'static str, &'static str>;
type Handle = mock::Handle<&'static str, &'static str>;

struct Exec;

impl<F> TypedExecutor<F> for Exec
where
    F: Future<Output = ()> + Send + 'static,
{
    fn spawn(&mut self, fut: F) -> Result<(), SpawnError> {
        thread::spawn(move || {
            let mut mock = tokio_test::task::MockTask::new();
            pin_mut!(fut);
            while mock.poll(fut.as_mut()).is_pending() {}
        });
        Ok(())
    }
}

struct ExecFn<Func>(Func);

impl<Func, F> TypedExecutor<F> for ExecFn<Func>
where
    Func: Fn(F) -> Result<(), ()>,
    F: Future<Output = ()> + Send + 'static,
{
    fn spawn(&mut self, fut: F) -> Result<(), SpawnError> {
        (self.0)(fut).map_err(|()| SpawnError::shutdown())
    }
}

fn new_service() -> (Buffer<Mock, &'static str>, Handle) {
    let (service, handle) = mock::pair();
    // bound is >0 here because clears_canceled_requests needs multiple outstanding requests
    let service = Buffer::with_executor(service, 10, &mut Exec);
    (service, handle)
}
