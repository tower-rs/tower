use super::MakeBalance;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::{fmt, marker::PhantomData};
use tower_layer::Layer;

/// Construct load balancers ([`Balance`]) over dynamic service sets ([`Discover`]) produced by the
/// "inner" service in response to requests coming from the "outer" service.
///
/// This construction may seem a little odd at first glance. This is not a layer that takes
/// requests and produces responses in the traditional sense. Instead, it is more like
/// [`MakeService`](tower::make_service::MakeService) in that it takes service _descriptors_ (see
/// `Target` on `MakeService`) and produces _services_. Since [`Balance`] spreads requests across a
/// _set_ of services, the inner service should produce a [`Discover`], not just a single
/// [`Service`], given a service descriptor.
///
/// See the [module-level documentation](..) for details on load balancing.
#[derive(Clone)]
pub struct MakeBalanceLayer<D, Req> {
    rng: SmallRng,
    _marker: PhantomData<fn(D, Req)>,
}

impl<D, Req> MakeBalanceLayer<D, Req> {
    /// Build balancers using operating system entropy.
    pub fn new() -> Self {
        Self {
            rng: SmallRng::from_entropy(),
            _marker: PhantomData,
        }
    }

    /// Build balancers using a seed from the provided random number generator.
    ///
    /// This may be preferrable when many balancers are initialized.
    pub fn from_rng<R: Rng>(rng: &mut R) -> Result<Self, rand::Error> {
        let rng = SmallRng::from_rng(rng)?;
        Ok(Self {
            rng,
            _marker: PhantomData,
        })
    }
}

impl<S, Req> Layer<S> for MakeBalanceLayer<S, Req> {
    type Service = MakeBalance<S, Req>;

    fn layer(&self, make_discover: S) -> Self::Service {
        MakeBalance::from_rng(make_discover, self.rng.clone()).expect("SmallRng is infallible")
    }
}

impl<D, Req> fmt::Debug for MakeBalanceLayer<D, Req> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MakeBalanceLayer")
            .field("rng", &self.rng)
            .finish()
    }
}
