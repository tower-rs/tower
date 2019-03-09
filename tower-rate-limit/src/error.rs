use std::error;

pub(crate) type Error = Box<error::Error + Send + Sync>;

pub(crate) mod never {
    use std::{error, fmt};

    #[derive(Debug)]
    pub enum Never {}

    impl fmt::Display for Never {
        fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
            unreachable!();
        }
    }

    impl error::Error for Never {}
}
