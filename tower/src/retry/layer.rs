use super::Retry;
use tower_layer::Layer;

/// Retry requests based on a policy
#[derive(Debug)]
pub struct RetryLayer<P> {
    policy: P,
}

impl<P> RetryLayer<P> {
    /// Create a new [`RetryLayer`] from a retry policy
    pub fn new(policy: P) -> Self {
        RetryLayer { policy }
    }
}

impl<P, S> Layer<S> for RetryLayer<P>
where
    P: Clone,
{
    type Service = Retry<P, S>;

    fn layer(&self, service: S) -> Self::Service {
        let policy = self.policy.clone();
        Retry::new(policy, service)
    }
}
