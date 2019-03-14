use crate::error::Never;
use crate::{Change, Discover};
use futures::{Async, Poll};
use std::iter::{Enumerate, IntoIterator};
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
{
    type Key = usize;
    type Service = U;
    type Error = Never;

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::Error> {
        match self.inner.next() {
            Some((i, service)) => Ok(Change::Insert(i, service).into()),
            None => Ok(Async::NotReady),
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
