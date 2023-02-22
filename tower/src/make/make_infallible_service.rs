use crate::sealed::Sealed;
use pin_project_lite::pin_project;
use std::future::Future;
use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

// TODO: documentation :)

pub trait MakeInfallibleService<S, F, Request>: Sealed<(Request,)> {
    fn make_infallible(self, err_into_ok: F) -> InfallibleService<S, F>;
}

impl<S, F, Request> MakeInfallibleService<S, F, Request> for S
where
    S: Service<Request>,
{
    fn make_infallible(self, err_into_ok: F) -> InfallibleService<S, F> {
        InfallibleService {
            service: self,
            err_into_ok,
        }
    }
}

#[derive(Debug)]
pub struct InfallibleService<S, F> {
    service: S,
    err_into_ok: F,
}

impl<S, F> Clone for InfallibleService<S, F>
where
    S: Clone,
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            err_into_ok: self.err_into_ok.clone(),
        }
    }
}

impl<R, S, F> Service<R> for InfallibleService<S, F>
where
    S: Service<R>,
    S::Future: Send,
    F: FnMut(S::Error) -> S::Response,
    F: Clone,
{
    type Response = S::Response;
    type Error = Infallible;
    type Future = InfallibleCall<F, S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.service.poll_ready(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(_)) => panic!("poll_ready must not return an error!"),
        }
    }

    fn call(&mut self, req: R) -> Self::Future {
        let fut = self.service.call(req);
        let err_into_ok = self.err_into_ok.clone();

        InfallibleCall { err_into_ok, fut }
    }
}

pin_project! {
    pub struct InfallibleCall<F, Fut> {
        err_into_ok: F,
        #[pin]
        fut: Fut,
    }
}

impl<F, Fut, T, E> Future for InfallibleCall<F, Fut>
where
    F: FnMut(E) -> T,
    Fut: Future<Output = Result<T, E>>,
{
    type Output = Result<T, Infallible>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = self.as_mut().project().fut.poll(cx);

        let this = self.as_mut().project();
        result.map(|r| Ok(r.unwrap_or_else(|err| (this.err_into_ok)(err))))
    }
}

#[cfg(test)]
mod tests {
    use super::MakeInfallibleService;
    use std::{
        future::{poll_fn, ready, Ready},
        task::Poll,
    };
    use tower_service::Service;

    #[tokio::test]
    #[should_panic(expected = "poll_ready must not return an error!")]
    async fn test_poll_ready_err_should_panic() {
        struct ReadyErrService;
        impl Service<String> for ReadyErrService {
            type Error = i32;
            type Response = String;
            type Future = Ready<Result<String, i32>>;

            fn poll_ready(
                &mut self,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), Self::Error>> {
                Poll::Ready(Err(-1))
            }

            fn call(&mut self, _: String) -> Self::Future {
                unreachable!()
            }
        }

        let mut service = ReadyErrService.make_infallible(|_: i32| unreachable!());
        let _ = poll_fn(|cx| service.poll_ready(cx)).await;
    }

    #[tokio::test]
    async fn test_call() {
        #[derive(Default)]
        struct FlakyGreeterService(i32);
        impl Service<String> for FlakyGreeterService {
            type Error = i32;
            type Response = String;
            type Future = Ready<Result<String, i32>>;

            fn poll_ready(
                &mut self,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), Self::Error>> {
                unreachable!("not part of the test")
            }

            fn call(&mut self, req: String) -> Self::Future {
                self.0 += 1;
                ready(if self.0 == 2 {
                    Err(-1)
                } else {
                    Ok(format!("Hello {}!", req))
                })
            }
        }

        let mut greeter_service =
            FlakyGreeterService::default().make_infallible(|err: i32| format!("Error {}", err));

        assert_eq!(
            greeter_service.call("Ferris".to_owned()).await,
            Ok("Hello Ferris!".to_owned())
        );
        assert_eq!(
            greeter_service.call("Bob".to_owned()).await,
            Ok("Error -1".to_owned())
        );
        assert_eq!(
            greeter_service.call("Rustaceans".to_owned()).await,
            Ok("Hello Rustaceans!".to_owned())
        );
    }
}
