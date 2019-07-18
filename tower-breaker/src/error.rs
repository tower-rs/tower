//! Error types

pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;
