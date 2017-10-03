extern crate futures;
extern crate tower;

use futures::Stream;
use tower::{NewService, Service};

use std::hash::Hash;

pub trait Discover: Stream {
    type Request;

    type Response;

    type ServiceError;

    type Service: Service<Request = Self::Request,
                         Response = Self::Response,
                            Error = Self::ServiceError>;
}

pub trait Key {
    type Key: Hash + Eq;

    fn key(&self) -> &Self::Key;
}

pub struct NewServiceSet<T> {
    _p: ::std::marker::PhantomData<T>,
}

impl<T, U> Discover for T
where U: NewService + Key,
      T: Stream<Item = NewServiceSet<U>>,
{
    type Request = U::Request;
    type Response = U::Response;
    type ServiceError = U::Error;
    type Service = U::Service;
}
