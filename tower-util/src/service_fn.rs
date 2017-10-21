use futures::{IntoFuture, Poll};
use tower::{Service, NewService};

use std::marker::PhantomData;

/// A `NewService` implemented by a closure.
pub struct NewServiceFn<T> {
    f: T,
}

// ===== impl NewServiceFn =====

impl<T, N> NewServiceFn<T>
where T: Fn() -> N,
      N: Service,
{
    /// Returns a new `NewServiceFn` with the given closure.
    pub fn new(f: T) -> Self {
        NewServiceFn { f }
    }
}

impl<T, R, S> NewService for NewServiceFn<T>
where T: Fn() -> R,
      R: IntoFuture<Item = S>,
      S: Service,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Service = R::Item;
    type InitError = R::Error;
    type Future = R::Future;

    fn new_service(&self) -> Self::Future {
        (self.f)().into_future()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{future, Future};
    use std::rc::Rc;

    #[test]
    fn smoke() {
        fn f<T>(service: &mut T)
        where T: Service<Request = u32,
                        Response = Rc<u32>,
                        Error = ()> + Sync
        {
            let resp = service.call(123);
            assert_eq!(*resp.wait().unwrap(), 456);
        }

        let mut service = ServiceFn::new(|request| {
            assert_eq!(request, 123);
            future::ok(Rc::new(456))
        });

        f(&mut service);
    }
}
