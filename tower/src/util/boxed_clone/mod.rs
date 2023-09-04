mod sync;
mod unsync;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::{
    sync::BoxCloneService, unsync::UnsyncBoxCloneService,
};
