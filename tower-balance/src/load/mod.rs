pub mod track;
mod constant;
mod pending_requests;

pub use self::track::Track;
pub use self::constant::Constant;
pub use self::pending_requests::{PendingRequests, WithPendingRequests};

/// Exposes a load metric.
///
/// Implementors should choose load values so that lesser-loaded instances return lesser
/// values than higher-load instances.
pub trait Load {
    type Metric;

    fn load(&self) -> Self::Metric;
}
