use futures::Future;
use tokio_io::{AsyncRead, AsyncWrite};
use tower_service::Service;

/// The MakeConnection trait is used to create transports
///
/// The goal of this service is to allow composable methods for creating
/// `AsyncRead + AsyncWrite` transports. This could mean creating a TLS
/// based connection or using some other method to authenticate the connection.
pub trait MakeConnection<Request> {
    /// The transport provided by this service
    type Response: AsyncRead + AsyncWrite;

    /// The extra data
    type Extra;

    /// Errors produced by the connecting service
    type Error;

    /// The future that eventually produces the transport
    type Future: Future<Item = (Self::Response, Self::Extra), Error = Self::Error>;

    /// Connect and return a transport asynchronously
    fn make_connection(&mut self, target: Request) -> Self::Future;
}

impl<S, Request> self::sealed::Sealed<Request> for S where S: Service<Request> {}

impl<C, Request, Response, Extra> MakeConnection<Request> for C
where
    C: Service<Request, Response = (Response, Extra)>,
    Response: AsyncRead + AsyncWrite,
{
    type Response = Response;
    type Extra = Extra;
    type Error = C::Error;
    type Future = C::Future;

    fn make_connection(&mut self, target: Request) -> Self::Future {
        Service::call(self, target)
    }
}

mod sealed {
    pub trait Sealed<A> {}
}
