mod instrument;
mod constant;
pub mod peak_ewma;
mod pending_requests;

pub use self::instrument::{Instrument, InstrumentFuture, NoInstrument};
pub use self::constant::Constant;
pub use self::peak_ewma::{PeakEwma, WithPeakEwma};
pub use self::pending_requests::{PendingRequests, WithPendingRequests};

/// Exposes a load metric.
///
/// Implementors should choose load values so that lesser-loaded instances return lesser
/// values than higher-load instances.
pub trait Load {
    type Metric;

    fn load(&self) -> Self::Metric;
}
