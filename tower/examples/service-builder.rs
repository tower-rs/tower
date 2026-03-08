//! An example demonstrating how to compose middleware using `ServiceBuilder`.
//!
//! This builds a service pipeline with multiple layers:
//! - `ConcurrencyLimit`: restricts the number of in-flight requests
//! - `RateLimit`: restricts the number of requests per time window
//! - `Timeout`: fails requests that take too long
//!
//! # Usage
//!
//! ```
//! cargo run --example service-builder --features full
//! ```

use std::time::Duration;
use tower::{Service, ServiceBuilder, ServiceExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();

    // Build a service that processes a string request by echoing it back.
    // Layers are added outside-in: the first layer added is the outermost.
    let mut svc = ServiceBuilder::new()
        // Allow at most 2 in-flight requests.
        .concurrency_limit(2)
        // Allow at most 5 requests per second.
        .rate_limit(5, Duration::from_secs(1))
        // Fail requests that take longer than 1 second.
        .timeout(Duration::from_secs(1))
        // The inner service: echoes the request back with a greeting.
        .service_fn(|request: String| async move {
            // Simulate some work.
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(format!("Hello, {request}!"))
        });

    // Send a few requests through the pipeline.
    for i in 1..=3 {
        // `ready` waits until the service is ready to accept a request.
        let svc = svc.ready().await.expect("service not ready");
        let response = svc.call(format!("request {i}")).await;
        match response {
            Ok(resp) => tracing::info!(%resp, "received response"),
            Err(err) => tracing::error!(%err, "request failed"),
        }
    }
}
