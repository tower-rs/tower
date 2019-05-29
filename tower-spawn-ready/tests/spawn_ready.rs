use futures::prelude::*;
use std::{thread, time::Duration};
use tokio_executor::{SpawnError, TypedExecutor};
use tower::Service;
use tower_spawn_ready::{error, SpawnReady};
use tower_test::mock;

#[test]
fn when_inner_is_not_ready() {
    let (mut service, mut handle) = new_service();

    // Make the service NotReady
    handle.allow(0);

    with_task(|| {
        let poll = service.poll_ready();
        assert!(poll.expect("poll_ready").is_not_ready());
    });

    // Make the service is Ready
    handle.allow(1);
    thread::sleep(Duration::from_millis(100));
    with_task(|| {
        let poll = service.poll_ready();
        assert!(poll.expect("poll_ready").is_ready());
    });
}

#[test]
fn when_inner_fails() {
    //use std::error::Error as StdError;

    let (mut service, mut handle) = new_service();

    // Make the service NotReady
    handle.allow(0);
    handle.send_error("foobar");

    with_task(|| {
        let e = service.poll_ready().unwrap_err();
        assert_eq!(e.to_string(), "foobar");
    });
}

#[test]
fn when_spawn_fails() {
    let (service, mut handle) = mock::pair::<(), ()>();

    let exec = ExecFn(|_| Err(()));
    let mut service = SpawnReady::with_executor(service, exec);

    // Make the service NotReady so a background task is spawned.
    handle.allow(0);

    let err = with_task(|| service.poll_ready().expect_err("poll_ready should error"));

    assert!(
        err.is::<error::SpawnError>(),
        "should be a SpawnError: {:?}",
        err
    );
}

type Mock = mock::Mock<&'static str, &'static str>;
type Handle = mock::Handle<&'static str, &'static str>;

struct Exec;

impl<F> TypedExecutor<F> for Exec
where
    F: Future<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, fut: F) -> Result<(), SpawnError> {
        thread::spawn(move || {
            fut.wait().unwrap();
        });
        Ok(())
    }
}

struct ExecFn<Func>(Func);

impl<Func, F> TypedExecutor<F> for ExecFn<Func>
where
    Func: Fn(F) -> Result<(), ()>,
    F: Future<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, fut: F) -> Result<(), SpawnError> {
        (self.0)(fut).map_err(|()| SpawnError::shutdown())
    }
}

fn new_service() -> (SpawnReady<Mock, &'static str, Exec>, Handle) {
    let (service, handle) = mock::pair();
    let service = SpawnReady::with_executor(service, Exec);
    (service, handle)
}

fn with_task<F: FnOnce() -> U, U>(f: F) -> U {
    use futures::future::lazy;
    lazy(|| Ok::<_, ()>(f())).wait().unwrap()
}
