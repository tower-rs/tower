//! Abstractions and utilties for measuring a service's load.

#![doc(html_root_url = "https://docs.rs/tower-load/0.1.0")]
#![deny(missing_docs)]
#![deny(rust_2018_idioms)]
#![deny(warnings)]
#![allow(elided_lifetimes_in_paths)]

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
