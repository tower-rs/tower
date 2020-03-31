//! Abstractions and utilties for measuring a service's load.

mod constant;
mod instrument;
pub mod peak_ewma;
pub mod pending_requests;

pub use self::{
    constant::Constant,
    instrument::{Instrument, InstrumentFuture, NoInstrument},
    peak_ewma::{PeakEwma, PeakEwmaDiscover},
    pending_requests::{PendingRequests, PendingRequestsDiscover},
};

/// Exposes a load metric.
pub trait Load {
    /// A comparable load metric. Lesser values are "preferable" to greater values.
    type Metric: PartialOrd;

    /// Obtains a service's load.
    fn load(&self) -> Self::Metric;
}
