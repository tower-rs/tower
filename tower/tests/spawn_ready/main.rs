#![cfg(feature = "spawn-ready")]
#[path = "../support.rs"]
mod support;

use std::task::{Context, Poll};
use tokio::time;
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok};
use tower::spawn_ready::{SpawnReady, SpawnReadyLayer};
use tower_test::mock;

#[tokio::test(flavor = "current_thread")]
async fn when_inner_is_not_ready() {
    time::pause();

    let _t = support::trace_init();

    let layer = SpawnReadyLayer::new();
    let (mut service, mut handle) = mock::spawn_layer::<(), (), _>(layer);

    // Make the service NotReady
    handle.allow(0);

    assert_pending!(service.poll_ready());

    // Make the service is Ready
    handle.allow(1);
    time::sleep(time::Duration::from_millis(100)).await;
    assert_ready_ok!(service.poll_ready());
}

#[tokio::test(flavor = "current_thread")]
async fn when_inner_fails() {
    let _t = support::trace_init();

    let layer = SpawnReadyLayer::new();
    let (mut service, mut handle) = mock::spawn_layer::<(), (), _>(layer);

    // Make the service NotReady
    handle.allow(0);
    handle.send_error("foobar");

    assert_eq!(
        assert_ready_err!(service.poll_ready()).to_string(),
        "foobar"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn propagates_trace_spans() {
    use futures::future;
    use std::fmt;
    use tower::{util::ServiceExt, Service};
    use tracing::Instrument;

    #[derive(Clone)]
    struct AssertSpanSvc {
        span: tracing::Span,
        polled: bool,
    }
    struct Error(String);

    impl fmt::Debug for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(&self.0, f)
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(&self.0, f)
        }
    }

    impl std::error::Error for Error {}

    impl AssertSpanSvc {
        fn check(&self, func: &str) -> Result<(), Error> {
            let current_span = tracing::Span::current();
            if current_span == self.span {
                return Ok(());
            }

            Err(Error(format!(
                "{} called outside expected span\n expected: {:?}\n  current: {:?}",
                func, self.span, current_span
            )))
        }
    }

    impl Service<()> for AssertSpanSvc {
        type Response = ();
        type Error = Error;
        type Future = future::Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            if self.polled {
                return Poll::Ready(self.check("poll_ready"));
            }

            cx.waker().wake_by_ref();
            self.polled = true;
            Poll::Pending
        }

        fn call(&mut self, _: ()) -> Self::Future {
            future::ready(self.check("call"))
        }
    }

    time::pause();
    let _t = support::trace_init();

    let span = tracing::info_span!("my_span");

    let service = AssertSpanSvc {
        span: span.clone(),
        polled: false,
    };

    let mut service = SpawnReady::new(service);
    let result =
        tokio::spawn(async move { service.ready_and().await?.call(()).await }.instrument(span));

    time::sleep(time::Duration::from_millis(100)).await;
    result.await.expect("service panicked").expect("failed");
}
