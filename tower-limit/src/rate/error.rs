use std::error;

pub(crate) type Error = Box<dyn error::Error + Send + Sync>;
