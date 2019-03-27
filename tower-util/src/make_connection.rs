use crate::sealed::Sealed;
use futures::{Future, Poll};
use tokio_io::{AsyncRead, AsyncWrite};
use tower_service::Service;

/// The MakeConnection trait is used to create transports
///
/// The goal of this service is to allow composable methods for creating
/// `AsyncRead + AsyncWrite` transports. This could mean creating a TLS
/// based connection or using some other method to authenticate the connection.
pub trait MakeConnection<Request>: Sealed<(Request,)> {
    /// The transport provided by this service
    type Connection: AsyncRead + AsyncWrite;

    /// Errors produced by the connecting service
    type Error;

    /// The future that eventually produces the transport
    type Future: Future<Item = Self::Connection, Error = Self::Error>;

    /// Returns `Ready` when it is able to make more connections.
    fn poll_ready(&mut self) -> Poll<(), Self::Error>;

    /// Connect and return a transport asynchronously
    fn make_connection(&mut self, target: Request) -> Self::Future;
}

impl<S, Request> Sealed<(Request,)> for S where S: Service<Request> {}

impl<C, Request> MakeConnection<Request> for C
where
    C: Service<Request>,
    C::Response: AsyncRead + AsyncWrite,
{
    type Connection = C::Response;
    type Error = C::Error;
    type Future = C::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Service::poll_ready(self)
    }

    fn make_connection(&mut self, target: Request) -> Self::Future {
        Service::call(self, target)
    }
}
