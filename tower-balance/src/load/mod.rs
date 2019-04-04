mod constant;
mod instrument;
pub mod peak_ewma;
pub mod pending_requests;
pub(crate) mod weight;

pub use self::constant::Constant;
pub use self::instrument::{Instrument, InstrumentFuture, NoInstrument};
pub use self::peak_ewma::{PeakEwma, WithPeakEwma};
pub use self::pending_requests::{PendingRequests, WithPendingRequests};

/// Exposes a load metric.
///
/// Implementors should choose load values so that lesser-loaded instances return lesser
/// values than higher-load instances.
pub trait Load {
    type Metric: PartialOrd;

    fn load(&self) -> Self::Metric;
}
