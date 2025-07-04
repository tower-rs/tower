/// # Buffer Example
///
/// This program demonstrates the use and benefits of the `Buffer` Service. `Buffer` makes it easy
/// to add multi-threaded access to an object that isn't normally sharable between threads. We
/// demonstrate this by using the `ServiceBuilder` to create a "stack" of services that wrap our
/// `SlowFibCalculator` example object.
use futures_core::Future;
use futures_util::{stream::FuturesUnordered, stream::StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration
};
use tokio::{task, time};
use tower::{
    buffer::BufferLayer, limit::ConcurrencyLimitLayer, Service, ServiceBuilder, ServiceExt,
};

struct SlowFibCalculator {
    delay: Duration,
    fib_prev: u64,
    fib_next: u64,
    request_count: u64,
}

/// `SlowFibCalculator` is an example `Service`.
///
/// Note that its primary method, `calculate` takes &mut self and struct itself is not Sync or
/// Send, so we certainly wouldn't be able to use this from multiple threads simultaneously.
impl SlowFibCalculator {
    pub fn new(delay: Duration) -> Self {
        SlowFibCalculator {
            fib_prev: 0,
            fib_next: 1,
            delay,
            request_count: 0,
        }
    }

    pub fn calculate(&mut self, iters: u64) -> impl Future<Output = Result<u64, String>> {
        let mut sum = self.fib_prev + self.fib_next;
        for _ in 0..iters {
            self.fib_prev = self.fib_next;
            self.fib_next = sum;
            sum = self.fib_prev + self.fib_next;
        }

        // We use an async block here instead of making the entire function async so that we don't
        // hold onto the mutable reference to `self` inside the future.
        let delay = self.delay;
        async move {
            time::sleep(delay).await;
            Ok(sum)
        }
    }
}

impl Service<u64> for SlowFibCalculator
{
    type Response = u64;
    type Error = String;
    type Future = Pin<Box<dyn Future<Output = Result<u64, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: u64) -> Self::Future {
        self.request_count += 1;

        Box::pin(self.calculate(request))
    }
}

#[tokio::main(worker_threads = 4)]
async fn main() {
    tracing::subscriber::set_global_default(tracing_subscriber::FmtSubscriber::default()).unwrap();

    // We're using the `ServiceBuilder` to compose our layers of `Service` calls. These are invoked
    // in order from first to last. We want `Buffer` to be first, since it provides the benefit of
    // making the entire stack `Clone`, `Sync`, and `Send`. After that, we include a
    // `ConcurrencyLimit` `Layer`, to ensure that consumers of our function can only invoke it one
    // at a time.
    //
    // Note that this is for demonstration purposes only, to exaggerate the serial nature of the
    // buffering process. Feel free to experiment with the `ConcurrencyLimit`` and `Buffer`
    // parameters and notice how it changes the parallelism (and execution time).
    //
    // Note also that `.layer(BufferLayer::new(10))` could more easily just be invoked by using
    // `.buffer(10)`, same with `ConcurrencyLimitLayer` could be invoked by
    // `.concurrency_limit(1)`. This is because `ServiceBuilder` has helper methods which wrap the
    // layer invocation for common utility services that are built into tower.
    let stack = ServiceBuilder::new()
        .layer(BufferLayer::new(10))
        .layer(ConcurrencyLimitLayer::new(1))
        .service(SlowFibCalculator::new(Duration::from_millis(300)));

    // Let's spin up 20 tasks and call our stack. Since these are potentially executing in separate
    // threads. Note that in the `tokio::main` macro, we are specifying 4 threads, so these tasks
    // may get spawned in any of 4 different threads, all competing to call our service first.
    let futures: FuturesUnordered<task::JoinHandle<_>> = (1..20u64)
        .map(|i| {
            // This `.clone()` only clones the top-level `Buffer` type which only clones an internally
            // held transmitter to the `mpsc` queue held by a single worker that was spawned when
            // creating the `Buffer` Service for the first time up in the `ServiceBuilder` up above.
            //
            // This means this clone is not a deep-clone but works similarly to how cloning an
            // `Arc<Mutex<T>>` returns a new pointer to the same type.
            let mut stack = stack.clone();

            tokio::spawn(async move {
                println!(
                    "{:?} - Task {} - Polling for ready",
                    std::thread::current().id(),
                    i
                );
                let service = stack.ready().await.unwrap();
                println!(
                    "{:?} - Task {} - Service is ready, calling",
                    std::thread::current().id(),
                    i
                );
                let result = service.call(1u64).await;

                (i, result.unwrap())
            })
        })
        .collect::<FuturesUnordered<_>>();

    // Note the interesting behavior of buffering. Approximately the first 10 requests reported
    // that they were "ready" fairly immediately. After that, the remaining tasks all polled for
    // ready but didn't get an immediate response. Also note that the tasks might execute out of
    // order, but the fib results *should* be returned in the correct order. See if you can effect
    // the fib order by tweaking the concurrency limit layer. Also see what the effect is of tweakig
    // the capacity of the buffer layer.
    futures.for_each(|result| async move {
        let res = result.unwrap();
        println!("Task: {} - Result: {}", res.0, res.1);
    })
    .await;
}
