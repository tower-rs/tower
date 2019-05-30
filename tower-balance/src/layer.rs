use crate::P2CBalance;
use rand::{rngs::SmallRng, FromEntropy, Rng, SeedableRng};
use std::{fmt, marker::PhantomData};
use tower_discover::Discover;
use tower_layer::Layer;

/// Efficiently distributes requests across an arbitrary number of services
#[derive(Clone)]
pub struct P2CBalanceLayer<D> {
    rng: SmallRng,
    _marker: PhantomData<fn(D)>,
}

impl<D> P2CBalanceLayer<D> {
    /// Builds a balancer using the system entropy.
    pub fn new() -> Self {
        Self {
            rng: SmallRng::from_entropy(),
            _marker: PhantomData,
        }
    }

    /// Builds a balancer from the provided RNG.
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

impl<D: Discover> Layer<D> for P2CBalanceLayer<D> {
    type Service = P2CBalance<D>;

    fn layer(&self, discover: D) -> Self::Service {
        P2CBalance::from_rng(discover, self.rng.clone())
    }
}

impl<D> fmt::Debug for P2CBalanceLayer<D> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("P2CBalanceLayer")
            .field("rng", &self.rng)
            .finish()
    }
}
