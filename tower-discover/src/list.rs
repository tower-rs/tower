use crate::{error::Never, Change, Discover};
use std::iter::{Enumerate, IntoIterator};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Static service discovery based on a predetermined list of services.
///
/// `ServiceList` is created with an initial list of services. The discovery
/// process will yield this list once and do nothing after.
pub struct ServiceList<T>
where
    T: IntoIterator,
{
    inner: Enumerate<T::IntoIter>,
}

impl<T, U> ServiceList<T>
where
    T: IntoIterator<Item = U>,
{
    pub fn new<Request>(services: T) -> ServiceList<T>
    where
        U: Service<Request>,
    {
        ServiceList {
            inner: services.into_iter().enumerate(),
        }
    }
}

impl<T, U> Discover for ServiceList<T>
where
    T: IntoIterator<Item = U>,
    Enumerate<T::IntoIter>: Unpin,
{
    type Key = usize;
    type Service = U;
    type Error = Never;

    fn poll(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Result<Change<Self::Key, Self::Service>, Self::Error>> {
        match self.inner.next() {
            Some((i, service)) => Poll::Ready(Ok(Change::Insert(i, service))),
            None => Poll::Pending,
        }
    }
}

// check that List can be directly over collections
#[cfg(test)]
#[allow(dead_code)]
type ListVecTest<T> = ServiceList<Vec<T>>;

#[cfg(test)]
#[allow(dead_code)]
type ListVecIterTest<T> = ServiceList<::std::vec::IntoIter<T>>;
