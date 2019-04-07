use crate::{Error, Timeout};
use crate::never::Never;
use std::time::Duration;
use tower_layer::Layer;
use tower_service::Service;
/// Applies a timeout to requests via the supplied inner service.
#[derive(Debug)]
pub struct TimeoutLayer {
    timeout: Duration,
}

impl TimeoutLayer {
    /// Create a timeout from a duration
    pub fn new(timeout: Duration) -> Self {
        TimeoutLayer { timeout }
    }
}

impl<S, Request> Layer<S, Request> for TimeoutLayer
where
    S: Service<Request>,
    Error: From<S::Error>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = Never;
    type Service = Timeout<S>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(Timeout::new(service, self.timeout))
    }
}
