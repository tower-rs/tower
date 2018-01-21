mod constant;
mod pending_requests;

pub use self::constant::Constant;
pub use self::pending_requests::{PendingRequests, WithPendingRequests};

/// Describes a relative load associated with an endpoint on [0.0, 1.0].
///
/// Values closer to 0.0 are considered "better" than values closer to 1.0.
#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Load(f64);

/// Exposes a `Load` metric.
///
/// Implementors should choose load values so that lesser-loaded instances return `Load`
/// instances with lesser values.
pub trait Loaded {
    fn load(&self) -> Load;
}

// ===== impl Load =====

impl Load {
    pub const MIN: Load = Load(0.0);
    pub const MAX: Load = Load(1.0);

    /// Creates a new `Load`.
    ///
    /// ## Panics
    ///
    /// If `n` is not on [0.0, 1.0].
    pub fn new(n: f64) -> Load {
        assert!(0.0 <= n && n <= 1.0, "load must be on [0.0, 1.0]");
        Load(n)
    }
}

/// Coerces a float value into a `Load`, rounding into the supported range as needed.
impl From<f64> for Load {
    fn from(n: f64) -> Self {
        if n < 0.0 {
            Load::MIN
        } else if n > 1.0 {
            Load::MAX
        } else {
            Load(n)
        }
    }
}

/// Extracts the underlying float value of a `Load` instance.
impl Into<f64> for Load {
    fn into(self) -> f64 {
        let Load(n) = self;
        n
    }
}
