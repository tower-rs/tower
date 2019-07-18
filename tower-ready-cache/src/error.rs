pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// An error indicating that the ready cache has no inner services.
#[derive(Debug)]
pub struct Exhausted;

/// An error indicating that the service with a `K`-typed key failed with an
/// error.
pub struct Failed<K>(pub K, pub Error);

// === Exhausted ===

impl std::fmt::Display for Exhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ready cache exhausted")
    }
}

impl std::error::Error for Exhausted {}

// === Failed ===

impl<K> std::fmt::Debug for Failed<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.1, f)
    }
}

impl<K> std::fmt::Display for Failed<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.1.fmt(f)
    }
}

impl<K> std::error::Error for Failed<K> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.1.source()
    }
}
