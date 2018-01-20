mod constant;
mod pending_requests;

pub use self::constant::Constant;
pub use self::pending_requests::PendingRequests;

pub trait Loaded {
    /// Returns a `Load` metric for the underlying `Service`, if the service is ready.
    ///
    /// Must not return a value unless `Service::poll_ready` would return `Async::Ready`.
    fn load(&self) -> Load;
}

/// Describes a relative load associated with an endpoint.
#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Load(f64);

// ===== impl Load =====

impl Load {
    pub const MIN: Load = Load(0.0);
    pub const MAX: Load = Load(1.0);

    pub fn new(n: f64) -> Load {
        assert!(0.0 <= n && n <= 1.0, "load must be on [0.0, 1.0]");
        Load(n)
    }
}

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
