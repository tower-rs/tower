use futures::{Future, Poll};
use tower::Service;

/// A boxed `Service` object.
///
/// `Boxed` turns a service into a trait object, allowing the response future
/// type to be dynamic.
#[derive(Debug)]
pub struct Boxed<S> {
    inner: S,
}

#[derive(Debug)]
struct BoxedUnsync<S> {
    inner: S,
}

// ===== impl Boxed =====

impl<S> Boxed<S>
where S: Service + 'static,
      S::Future: 'static,
{
    /// Return a new boxed `Service + Send`.
    ///
    /// Both the service AND the response future must be *Send*. This allows the
    /// returned service object to be moved across threads.
    pub fn new(inner: S)
        -> Box<Service<Request = S::Request,
                      Response = S::Response,
                         Error = S::Error,
                        Future = Box<Future<Item = S::Response,
                                           Error = S::Error> + Send>> + Send>
    where S: Send,
          S::Future: Send,
    {
        Box::new(Boxed { inner })
    }

    /// Returns a new boxed `Service`.
    ///
    /// The returned service is **not** `Send` and must remain on the thread
    /// that it got constructed on.
    pub fn unsync(inner: S)
        -> Box<Service<Request = S::Request,
                      Response = S::Response,
                         Error = S::Error,
                        Future = Box<Future<Item = S::Response,
                                           Error = S::Error>>>>
    {
        Box::new(BoxedUnsync { inner })
    }
}

impl<S> Service for Boxed<S>
where S: Service + 'static,
      S::Future: Send + 'static,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = Box<Future<Item = S::Response,
                            Error = S::Error> + Send>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        Box::new(self.inner.call(request))
    }
}

// ===== impl BoxedUnsync =====

impl<S> Service for BoxedUnsync<S>
where S: Service + 'static,
      S::Future: 'static,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = Box<Future<Item = S::Response,
                            Error = S::Error>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        Box::new(self.inner.call(request))
    }
}
