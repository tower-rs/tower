//! A Power-of-Two-Choices Load Balancer

mod layer;
mod make;
mod service;

#[cfg(test)]
mod test;

pub use layer::BalanceLayer;
pub use make::{MakeFuture, BalanceMake};
pub use service::Balance;
