use std::error;
use std::fmt;

#[derive(Debug)]
pub struct None(());

pub(crate) type Error = Box<dyn error::Error + Send + Sync>;

impl None {
    pub(crate) fn new() -> None {
        None(())
    }
}

impl fmt::Display for None {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "None")
    }
}

impl error::Error for None {}
