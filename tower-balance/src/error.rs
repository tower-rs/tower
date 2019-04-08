use std::fmt;

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
pub struct Balance(pub(crate) Error);

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "load balancing discover error: {}", self.0)
    }
}

impl std::error::Error for Balance {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}
