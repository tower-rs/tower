use super::error::{never::Never, Error};
use super::{Rate, LimitRate};
use std::time::Duration;
use tower_layer::Layer;
use tower_service::Service;

#[derive(Debug)]
pub struct LimitRateLayer {
    rate: Rate,
}

impl LimitRateLayer {
    pub fn new(num: u64, per: Duration) -> Self {
        let rate = Rate::new(num, per);
        LimitRateLayer { rate }
    }
}

impl<S, Request> Layer<S, Request> for LimitRateLayer
where
    S: Service<Request>,
    Error: From<S::Error>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = Never;
    type Service = LimitRate<S>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(LimitRate::new(service, self.rate))
    }
}
