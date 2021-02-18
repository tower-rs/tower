//! TODO(david): docs

mod layer;
mod picker;
mod service;

pub use self::{
    layer::MultiplexLayer,
    picker::{Pick, Picker, not, And, Or},
    service::{Multiplex, ResponseFuture},
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{service_fn, BoxError, Service, ServiceBuilder, ServiceExt};

    #[tokio::test]
    async fn basic() {
        let mut svc = ServiceBuilder::new()
            .multiplex(service_fn(one), pick_if(1))
            .multiplex(service_fn(two), pick_if(2))
            .multiplex(service_fn(three), pick_if(3))
            .service(service_fn(bottom));

        assert_eq!(svc.ready_and().await.unwrap().call(1).await.unwrap(), "one",);

        assert_eq!(svc.ready_and().await.unwrap().call(2).await.unwrap(), "two",);

        assert_eq!(
            svc.ready_and().await.unwrap().call(3).await.unwrap(),
            "three",
        );

        assert_eq!(
            svc.ready_and().await.unwrap().call(4).await.unwrap(),
            "bottom",
        );
    }

    fn pick_if(n: i32) -> impl Picker<i32> + Clone {
        move |req: &mut i32| {
            if n == *req {
                Pick::First
            } else {
                Pick::Second
            }
        }
    }

    async fn one(req: i32) -> Result<&'static str, BoxError> {
        assert_eq!(req, 1);
        Ok("one")
    }

    async fn two(req: i32) -> Result<&'static str, BoxError> {
        assert_eq!(req, 2);
        Ok("two")
    }

    async fn three(req: i32) -> Result<&'static str, BoxError> {
        assert_eq!(req, 3);
        Ok("three")
    }

    async fn bottom(_req: i32) -> Result<&'static str, BoxError> {
        Ok("bottom")
    }
}
