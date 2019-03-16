use std::error::Error;
use std::fmt;

#[derive(Debug)]
/// An error that can never occur.
pub enum Never {}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
        match *self {}
    }
}

impl Error for Never {}
