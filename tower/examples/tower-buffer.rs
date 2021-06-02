/// Buffer Example
/// This program demonstrates the use and benefits of the Buffer Service.
/// Buffer makes it easy to add multi-thread access to an object that isn't normally
/// sharable between threads.
/// We demonstrate this by using the `ServiceBuilder` to create a "stack" of services
/// that wrap our SlowFibCalculator example object.
use futures_core::Future;
use futures_util::{stream::FuturesUnordered, stream::StreamExt};
use num_traits::PrimInt;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration
};
use tokio::{task, time};
use tower::{
    buffer::BufferLayer, limit::ConcurrencyLimitLayer, Service, ServiceBuilder, ServiceExt,
};

/// The SlowFibCalculator struct
struct SlowFibCalculator<N: PrimInt + Send> {
    delay: Duration,
    fib_prev: N,
    fib_next: N,
    requests: u64,
    responses: u64,
}

/// SlowFibCalculator is an Example [Service]
/// Note that its primary method, `calculate` takes &mut self
/// and struct itself is not Sync or Send, so we certainly
/// wouldn't be able to use this from multiple threads simultaneously
impl<N: PrimInt + Send> SlowFibCalculator<N> {
    pub fn new(delay: Duration) -> Self {
        SlowFibCalculator {
            fib_prev: N::zero(),
            fib_next: N::one(),
            delay,
            requests: 0,
            responses: 0,
        }
    }

    /// This is an overly elaborate fibonacci calculator Future
    /// It is generic over all Ints
    /// It also takes a param to delay some amount between iterations
    pub fn calculate(&mut self, iters: N) -> impl Future<Output = Result<N, String>> {
        let delay = self.delay;
        let n = iters.to_usize().unwrap();
        let mut res = self.fib_prev + self.fib_next;
        println!("calculating...");
        for _ in 0..n {
            self.fib_prev = self.fib_next;
            self.fib_next = res;
            res = self.fib_prev + self.fib_next;
        }
        async move {
            time::sleep(delay).await;
            Ok(res)
        }
    }
}

/// Implementing the [Service] trait for `SlowFibCalculator`
/// Since the `calculate` method's argument and return type is generic,
/// this impl must be as well.
impl<N> Service<N> for SlowFibCalculator<N>
where
    N: PrimInt + Send + 'static,
{
    type Response = N;
    type Error = String;
    type Future = Pin<Box<dyn Future<Output = Result<N, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: N) -> Self::Future {
        self.requests = self.requests + 1;
        let res = self.calculate(req);
        self.responses = self.responses + 1;
        Box::pin(res)
    }
}

#[tokio::main(worker_threads = 4)]
async fn main() {
    tracing::subscriber::set_global_default(tracing_subscriber::FmtSubscriber::default()).unwrap();

    // We're using the ServiceBuilder to compose our layers of Service calls. These are invoked in
    // order from first to last. We want Buffer to be first, since it provides the benefit of making
    // the entire stack Clone, Sync, and Send.
    // After that, we include ConcurrencyLimit Layer, to ensure that consumers of our function can
    // only invoke it one at a time.
    // Note that this is for demonstration purposes only, to exagerate the serial nature of the buffering
    // process.  Feel free to experiment with the ConcurrencyLimit and Buffer parameters and notice
    // how it changes the parallelism (and execution time)
    //
    // Note also that .layer(BufferLayer::new(10)) could more easily just be invoked by using
    // .buffer(10), same with ConcurrencyLimitLayer could be invoked by .concurrency_limit(1)
    // This is because [ServiceBuilder] has helper methods which wrap the layer invocation for
    // common utility Services that are built into Tower
    let stack = ServiceBuilder::new()
        .layer(BufferLayer::new(10))
        .layer(ConcurrencyLimitLayer::new(1))
        .service(SlowFibCalculator::new(Duration::from_millis(20)));

    // Let's spin up 20 tasks and call our stack. Since these are potentially executing in separate threads
    // Note that in the tokio::main macro, we are specifying 4 threads, so these tasks may get
    // spawned in any of 4 different threads, all competing to call our service first.
    let futs: FuturesUnordered<task::JoinHandle<_>> = (1..20u64)
        .map(|i| {
            let mut stack = stack.clone();
            tokio::spawn(async move {
                println!(
                    "ThreadId: {:?} - Task {} - Polling for ready",
                    std::thread::current().id(),
                    i
                );
                let svc = stack.ready().await.unwrap();
                println!(
                    "ThreadId: {:?} - Task {} - Svc is ready, calling",
                    std::thread::current().id(),
                    i
                );
                let res = svc.call(1u64).await;
                (i, res.unwrap())
            })
        })
        .collect::<FuturesUnordered<_>>();

    println!("\n############\nGot results: ");
    // Note the interesting behavior of buffering. Approximately the first 10 requests reported
    // that they were "ready" fairly immediately. After that, the remaining tasks all polled for
    // ready but didn't get an immediate response.
    // Also note that the tasks might execute out of order, but the fib results *should* be
    // returned in the correct order.
    // See if you can effect the fib order by tweaking the concurrency limit
    futs.for_each(|result| async move {
        let res = result.unwrap();
        println!("Task: {} - Result: {}", res.0, res.1);
    })
    .await;
}
