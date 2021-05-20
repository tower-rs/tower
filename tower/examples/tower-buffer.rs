/// Buffer Example
/// This program demonstrates the use and benefits of the Buffer Service.
/// Buffer makes it easy to add multi-thread access to an object that isn't normally
/// sharable between threads.
/// We demonstrate this by using the `ServiceBuilder` to create a "stack" of services
/// that wrap our SlowFibCalculator example object.
use futures_core::{future::BoxFuture, Future, Stream, TryStream};
use futures_util::future::{self, FutureExt, Lazy};
use futures_util::{stream, stream::StreamExt, stream::TryStreamExt};
use num_traits::{PrimInt, ToPrimitive};
use pin_project::pin_project;
use rand::{self, Rng};
use std::hash::Hash;
use std::time::Duration;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{task, time};
use tower::{
    buffer::BufferLayer, limit::ConcurrencyLimitLayer, Service, ServiceBuilder, ServiceExt,
};

/// The SlowFibCalculator struct
struct SlowFibCalculator {
    delay: Duration,
    requests: usize,
    responses: usize,
}

/// SlowFibCalculator is an Example [Service]
/// Note that its primary method, `calculate` takes &mut self
/// and struct itself is not Sync or Send, so we certainly
/// wouldn't be able to use this from multiple threads simultaneously
impl SlowFibCalculator {
    pub fn new(delay: Duration) -> Self {
        SlowFibCalculator {
            requests: 0,
            responses: 0,
            delay,
        }
    }

    /// This is an overly elaborate fibonacci calculator Future
    /// It is generic over all Ints
    /// It also takes a param to delay some amount between iterations
    pub fn calculate<N: PrimInt + Send>(
        &mut self,
        iters: N,
    ) -> impl Future<Output = Result<N, String>> {
        let mut x = N::zero();
        let mut y = N::one();
        let mut z = N::one();
        let delay = self.delay;
        let n = iters.to_usize().unwrap();
        println!("calculating...");
        async move {
            for _ in 0..n {
                x = y;
                y = z;
                z = x + y;
                time::sleep(delay).await;
            }
            Ok(x)
        }
    }
}

/// Implementing the [Service] trait for `SlowFibCalculator`
/// Since the `calculate` method's argument and return type is generic,
/// this impl must be as well.
impl<I> Service<I> for SlowFibCalculator
where
    I: PrimInt + Send + 'static,
{
    type Response = I;
    type Error = String;
    type Future = Pin<Box<Future<Output = Result<I, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: I) -> Self::Future {
        self.requests = self.requests + 1;
        let res = self.calculate(req);
        self.responses = self.responses + 1;
        Box::pin(res)
    }
}

#[tokio::main]
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
        .layer(ConcurrencyLimitLayer::new(2))
        .service(SlowFibCalculator::new(Duration::from_millis(5)));

    // Let's spin up 20 tasks and call our stack. Since these are potentially executing in separate threads
    // Note that the Tokio Runtime, specified by `tokio::main` defaults to a multi-threaded runtime.
    // So these tasks are going to be spawned from multiple threads. Note that we haven't had to do
    // anything special in order to clone `stack` and use it from multiple threads simultaneously.
    let futs = (10..30_u64)
        .map(|i| {
            let mut stack = stack.clone();
            tokio::spawn(async move {
                println!("Task {} - Polling for ready", i);
                let svc = stack.ready().await.unwrap();
                println!("Task {} - Svc is ready, calling", i);
                svc.call(i).await
            })
        })
        .collect::<Vec<task::JoinHandle<_>>>();

    // After invoking all of the tasks, we wait on their results
    let results = future::join_all(futs).await;

    println!("\n############\nGot results: ");
    results
        .iter()
        .for_each(|r| println!("{}", r.as_ref().unwrap().as_ref().unwrap()));
}
