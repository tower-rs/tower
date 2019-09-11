use futures_util::pin_mut;
use std::{future::Future, thread, time::Duration};
use tokio_executor::{Executor, SpawnError};
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok, task};
use tower_service::Service;
use tower_spawn_ready::{error, SpawnReady};
use tower_test::mock;

#[test]
fn when_inner_is_not_ready() {
    let mut ex = Exec;
    tokio_executor::with_default(&mut ex, || {
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

        let mut service = SpawnReady::new(service);

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

impl Executor for Exec {
    fn spawn(
        &mut self,
        fut: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<(), SpawnError> {
        thread::spawn(move || {
            let mut mock = tokio_test::task::MockTask::new();
            pin_mut!(fut);
            while mock.poll(fut.as_mut()).is_pending() {}
        });
        Ok(())
    }
}

fn new_service() -> (SpawnReady<Mock>, Handle) {
    let (service, handle) = mock::pair();
    let service = SpawnReady::new(service);
    (service, handle)
}
