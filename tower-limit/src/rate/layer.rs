use super::{Rate, RateLimit};
use std::time::Duration;
use tower_layer::Layer;

#[derive(Debug)]
pub struct RateLimitLayer {
    rate: Rate,
}

impl RateLimitLayer {
    pub fn new(num: u64, per: Duration) -> Self {
        let rate = Rate::new(num, per);
        RateLimitLayer { rate }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimit<S>;

    fn layer(&self, service: S) -> Self::Service {
        RateLimit::new(service, self.rate)
    }
}
