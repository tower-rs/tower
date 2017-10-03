extern crate futures;
extern crate tower;

use futures::Stream;
use tower::{NewService, Service};

use std::net::SocketAddr;

pub trait Discover: Stream {
    type Request;

    type Response;

    type ServiceError;

    type Service: Service<Request = Self::Request,
                         Response = Self::Response,
                            Error = Self::ServiceError>;
}

pub trait Destination {
    fn destination(&self) -> &SocketAddr;
}

pub struct NewServiceSet<T> {
    _p: ::std::marker::PhantomData<T>,
}

impl<T, U> Discover for T
where U: NewService + Destination,
      T: Stream<Item = NewServiceSet<U>>,
{
    type Request = U::Request;
    type Response = U::Response;
    type ServiceError = U::Error;
    type Service = U::Service;
}
