use futures_util::pin_mut;
use std::{future::Future, thread, time::Duration};
use tokio_executor::{SpawnError, TypedExecutor};
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok, task};
use tower_service::Service;
use tower_spawn_ready::{error, SpawnReady};
use tower_test::mock;

#[test]
fn when_inner_is_not_ready() {
    task::mock(|cx| {
        let (mut service, handle) = new_service();
        pin_mut!(handle);

        // Make the service NotReady
        handle.allow(0);

        assert_pending!(service.poll_ready(cx));

        // Make the service is Ready
        handle.allow(1);
        thread::sleep(Duration::from_millis(100));
        assert_ready_ok!(service.poll_ready(cx));
    });
}

#[test]
fn when_inner_fails() {
    task::mock(|cx| {
        let (mut service, handle) = new_service();
        pin_mut!(handle);

        // Make the service NotReady
        handle.allow(0);
        handle.send_error("foobar");

        assert_eq!(
            assert_ready_err!(service.poll_ready(cx)).to_string(),
            "foobar"
        );
    });
}

#[test]
fn when_spawn_fails() {
    task::mock(|cx| {
        let (service, handle) = mock::pair::<(), ()>();
        pin_mut!(handle);

        let exec = ExecFn(|_| Err(()));
        let mut service = SpawnReady::with_executor(service, exec);

        // Make the service NotReady so a background task is spawned.
        handle.allow(0);

        let err = assert_ready_err!(service.poll_ready(cx));

        assert!(
            err.is::<error::SpawnError>(),
            "should be a SpawnError: {:?}",
            err
        );
    });
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

fn new_service() -> (SpawnReady<Mock, Exec>, Handle) {
    let (service, handle) = mock::pair();
    let service = SpawnReady::with_executor(service, Exec);
    (service, handle)
}
