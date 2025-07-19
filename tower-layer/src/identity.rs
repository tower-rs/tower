use super::Layer;
use core::fmt;

/// A no-op middleware.
///
/// When wrapping a [`Service`], the [`Identity`] layer returns the provided
/// service without modifying it.
///
/// [`Service`]: tower_service::Service
///
/// # Examples
///
/// ```rust
/// use tower_layer::Identity;
/// use tower_layer::Layer;
///
/// let identity = Identity::new();
///
/// assert_eq!(identity.layer(42), 42);
/// ```
#[derive(Default, Clone)]
pub struct Identity {
    _p: (),
}

impl Identity {
    /// Creates a new [`Identity`].
    ///
    /// ```rust
    /// # tokio_test::block_on(async {
    /// use tower_layer::Identity;
    ///
    /// let identity = Identity::new();
    /// # })
    /// ```
    pub const fn new() -> Identity {
        Identity { _p: () }
    }
}

impl<S> Layer<S> for Identity {
    type Service = S;

    fn layer(&self, inner: S) -> Self::Service {
        inner
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Identity").finish()
    }
}
