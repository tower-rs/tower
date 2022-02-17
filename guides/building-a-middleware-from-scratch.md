# Building a middleware from scratch

In ["Inventing the `Service` trait"][invent] we learned all the motivations
behind [`Service`] and why its designed the way it is. We also built a few
smaller middleware ourselves but we took a few shortcuts in our implementation.
In this guide we're going to build the `Timeout` middleware as it exists in
Tower today without taking any shortcuts.

Writing a robust middleware requires working with async Rust at a slightly lower
level than you might be used to. The goal of this guide is to demystify the
concepts and patterns so you can start writing your own middleware and maybe
even contribute back to the Tower ecosystem!

## Getting started

The middleware we're going to build is [`tower::timeout::Timeout`]. It will set
a limit on the maximum duration its inner `Service`'s response future is allowed
to take. If it doesn't produce a response within some amount of time, an error
is returned. This allows the client to retry that request or report an error to
the user, rather than waiting forever.

Lets start by writing a `Timeout` struct that holds the `Service` its wrapping
and the duration of the timeout:

```rust
use std::time::Duration;

struct Timeout<T> {
    inner: T,
    timeout: Duration,
}
```

As we learned in ["Inventing the `Service` trait"][invent] its important for
services to implement `Clone` such that you can convert the `&mut self` given to
`Service::call` into an owned `self` that can be moved into the response future,
if necessary. We should therefore add `#[derive(Clone)]` to our struct. We
should also derive `Debug` while we're at it:

```rust
#[derive(Debug, Clone)]
struct Timeout<S> {
    inner: S,
    timeout: Duration,
}
```

Next we write a constructor:

```rust
impl<S> Timeout<S> {
    pub fn new(inner: S, timeout: Duration) -> Self {
        Timeout { inner, timeout }
    }
}
```

Note that we omit bounds on S even though we expect it to implement Service, as
the [Rust API guidelines recommend][rust-guidelines].

Now the interesting bit. How to implement `Service` for `Timeout<S>`? Lets start
with an implementation that just forwards everything to the inner service:

```rust
use tower::Service;
use std::task::{Context, Poll};

impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Our middleware doesn't care about backpressure so its ready as long
        // as the inner service is ready.
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request)
    }
}
```

Until you've written lots of middleware writing out a skeleton like this makes
the process a bit easier.

To actually add a timeout to the inner service what we essentially have to do is
detect when the future returned by `self.inner.call(request)` has been running
longer than `self.duration` and abort with an error.

The approach we're going to take is to call [`tokio::time::sleep`] to get a
future that completes when we're out of time and then select the value from
whichever of the two futures is the first to complete. We could also use
`tokio::time::timeout` but `sleep` works just as well.

Creating both futures is done like this:

```rust
use tokio::time::sleep;

fn call(&mut self, request: Request) -> Self::Future {
    let response_future = self.inner.call(request);

    // This variable has type `tokio::time::Sleep`.
    //
    // We don't have to clone `self.duration` as it implements the `Copy` trait.
    let sleep = tokio::time::sleep(self.timeout);

    // what to write here?
}
```

One possible return type is `Pin<Box<dyn Future<...>>>`. However we want our
`Timeout` to add as little overhead as possible, so we would like to find a way
to avoid allocating a `Box`. Imagine we have a large stack, with dozens of
nested `Service`s, where each layer allocates a new `Box` for every request
that passes through it. That would result in a lot of allocations which might
impact performance[^1].

## The response future

To avoid using `Box` lets instead write our own `Future` implementation. We
start by creating a struct called `ResponseFuture`. It has to be generic over
the inner service's response future type. This is analogous to wrapping services
in other services, but this time we're wrapping futures in other futures.

```rust
use tokio::time::Sleep;

pub struct ResponseFuture<F> {
    response_future: F,
    sleep: Sleep,
}
```

`F` will be the type of `self.inner.call(request)`. Updating our `Service`
implementation we get:

```rust
impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;

    // Use our new `ResponseFuture` type.
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response_future = self.inner.call(request);
        let sleep = tokio::time::sleep(self.timeout);

        // Create our response future by wrapping the future from the inner
        // service.
        ResponseFuture {
            response_future,
            sleep,
        }
    }
}
```

A key point here is that Rust's futures are _lazy_. That means nothing actually
happens until they're `await`ed or polled. So `self.inner.call(request)` will
return immediately without actually processing the request.

Next we go ahead and implement `Future` for `ResponseFuture`:

```rust
use std::{pin::Pin, future::Future};

impl<F, Response, Error> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response, Error>>,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // What to write here?
    }
}
```

Ideally we want to write something like this:

1. First poll `self.response_future` and if its ready return the response or error it
   resolved to.
2. Otherwise poll `self.sleep` and if its ready return an error.
3. If neither future is ready return `Poll::Pending`.

We might try:

```rust
fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    match self.response_future.poll(cx) {
        Poll::Ready(result) => return Poll::Ready(result),
        Poll::Pending => {}
    }

    todo!()
}
```

However that gives an error like this:

```
error[E0599]: no method named `poll` found for type parameter `F` in the current scope
  --> src/lib.rs:56:29
   |
56 |         match self.response_future.poll(cx) {
   |                             ^^^^ method not found in `F`
   |
   = help: items from traits can only be used if the type parameter is bounded by the trait
help: the following traits define an item `poll`, perhaps you need to restrict type parameter `F` with one of them:
   |
49 | impl<F: Future, Response, Error> Future for ResponseFuture<F>
   |      ^^^^^^^^^

error: aborting due to previous error
```

Unfortunately the error we get from Rust isn't very good. It tells us to add an
`F: Future` bound even though we've already done that with `where F:
Future<Output = Result<Response, E>>`.

The real issue has to do with [`Pin`]. The full details of pinning is outside
the scope of this guide. If you're new to `Pin` we recommend ["The Why, What,
and How of Pinning in Rust"][pin] by Jon Gjengset.

What Rust is trying to tell us is that we need a `Pin<&mut F>` to be able to
call `poll`. Accessing `F` through `self.response_future` when `self` is a
`Pin<&mut Self>` doesn't work.

What we need is called "pin projection" which means going from a `Pin<&mut
Struct>` to a `Pin<&mut Field>`. Normally pin projection would require writing
`unsafe` code but the excellent [pin-project] crate is able to handle all the
`unsafe` details for us.

Using pin-project we can annotate a struct with `#[pin_project]` and add
`#[pin]` to each field that we want to be able to access through a pinned
reference:

```rust
use pin_project::pin_project;

#[pin_project]
pub struct ResponseFuture<F> {
    #[pin]
    response_future: F,
    #[pin]
    sleep: Sleep,
}

impl<F, Response, Error> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response, Error>>,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Call the magical `project` method generated by `#[pin_project]`.
        let this = self.project();

        // `project` returns a `__ResponseFutureProjection` but we can ignore
        // the exact type. It has fields that matches `ResponseFuture` but
        // maintain pins for fields annotated with `#[pin]`.

        // `this.response_future` is now a `Pin<&mut F>`.
        let response_future: Pin<&mut F> = this.response_future;

        // And `this.sleep` is a `Pin<&mut Sleep>`.
        let sleep: Pin<&mut Sleep> = this.sleep;

        // If we had another field that wasn't annotated with `#[pin]` that
        // would have been a regular `&mut` without `Pin`.

        // ...
    }
}
```

Pinning in Rust is a complex topic that is hard to understand but thanks to
pin-project we're able to ignore most of that complexity. Crucially, it means we
don't have to fully understand pinning to write Tower middleware. So if you
didn't quite get all the stuff about `Pin` and `Unpin` fear not because
pin-project has your back!

Notice in the previous code block we were able to obtain a `Pin<&mut F>` and a
`Pin<&mut Sleep>` which is exactly what we need to call `poll`:

```rust
impl<F, Response, Error> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response, Error>>,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // First check if the response future is ready.
        match this.response_future.poll(cx) {
            Poll::Ready(result) => {
                // The inner service has a response ready for us or it has
                // failed.
                return Poll::Ready(result);
            }
            Poll::Pending => {
                // Not quite ready yet...
            }
        }

        // Then check if the sleep is ready. If so the response has taken too
        // long and we have to return an error.
        match this.sleep.poll(cx) {
            Poll::Ready(()) => {
                // Our time is up, but error do we return?!
                todo!()
            }
            Poll::Pending => {
                // Still some time remaining...
            }
        }

        // If neither future is ready then we are still pending.
        Poll::Pending
    }
}
```

Now the only remaining question is what error should we return if the sleep
finishes first?

## The error type

The error type we're promising to return now is the generic `Error` type which
is the same as the inner service's error type. However we know nothing about
that type. It is completely opaque to us and we have no way of constructing
values of that type.

We have three options:

1. Return a boxed error trait object like `Box<dyn std::error::Error + Send +
   Sync>`.
2. Return an enum with variants for the service error and the timeout error.
3. Define a `TimeoutError` struct and require that our generic error type can be
   constructed from a `TimeoutError` using `TimeoutError: Into<Error>`.

While option 3 might seem like it is the most flexible it isn't great since it
requires users using a custom error type to manually implement
`From<TimeoutError> for MyError`. That quickly becomes tedious when using lots
of middleware that each have their own error type.

Option 2 would mean defining an enum like this:

```rust
enum TimeoutError<Error> {
    // Variant used if we hit the timeout
    Timeout(InnerTimeoutError),
    // Variant used if the inner service produced an error
    Service(Error),
}
```

While this seems ideal on the surface as we're not losing any type information
and can use `match` to get at the exact error, the approach has three issues:

1. In practice its common to nest lots of middleware. That would make the final
   error enum very large. Its not unlikely to look something like
   `BufferError<RateLimitError<TimeoutError<MyError>>>`. Pattern matching on
   such a type (to for example determine if the error is retry-able) is very
   tedious.
2. If we change the order our middleware are applied in we also change the final
   error type meaning we have to update our pattern matches.
3. There is also the possibility of the final error type being very large and
   taking up a significant amount of space on the stack.

With this we're left with option 1 which is to convert the inner service error
into a boxed trait object like `Box<dyn std::error::Error + Send + Sync>`. That
means we can combine multiple errors type into one. That has the following
advantages:

1. Our error handling is less fragile since changing the order middleware are
   applied in wont change the final error type.
2. The error type now has a constant size regardless how many middleware we've
   applied.
3. Extracting the error no longer requires a big `match` but can instead be done
   with `error.downcast_ref::<Timeout>()`.

However it also has the following downsides:

1. As we're using dynamic downcasting the compiler can no longer guarantee that
   we're exhaustively checking for every possible error type.
2. Creating an error now requires an allocation. In practice we expect errors to
   be infrequent and therefore this shouldn't be a problem.

Which option you prefer is a matter of personal preference. Both have their
advantages and disadvantages. However the pattern that we've decided to use in
Tower is boxed trait objects. You can find the original discussion
[here](https://github.com/tower-rs/tower/issues/131).

For our `Timeout` middleware that means we need to create a struct that
implements `std::error::Error` such that we can convert it into a `Box<dyn
std::error::Error + Send + Sync>`. We also have to require that the inner
service's error type implements `Into<Box<dyn std::error::Error + Send +
Sync>>`. Luckily most errors automatically satisfies that so it wont require
users to write any additional code. We're using `Into` for the trait bound
rather than `From` as recommend by the [standard
library](https://doc.rust-lang.org/stable/std/convert/trait.From.html).

The code for our error type looks like this:

```rust
use std::fmt;

#[derive(Debug, Default)]
pub struct TimeoutError(());

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("request timed out")
    }
}

impl std::error::Error for TimeoutError {}
```

We add a private field to `TimeoutError` such that users outside of Tower cannot
construct their own `TimeoutError`. They can only be obtained through our
middleware.

`Box<dyn std::error::Error + Send + Sync>` is also quite a mouthful so
lets define a type alias for it:

```rust
// This also exists as `tower::BoxError`
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
```

Our future implementation now becomes:

```rust
impl<F, Response, Error> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response, Error>>,
    // Require that the inner service's error can be converted into a `BoxError`.
    Error: Into<BoxError>,
{
    type Output = Result<
        Response,
        // The error type of `ResponseFuture` is now `BoxError`.
        BoxError,
    >;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.response_future.poll(cx) {
            Poll::Ready(result) => {
                // Use `map_err` to convert the error type.
                let result = result.map_err(Into::into);
                return Poll::Ready(result);
            }
            Poll::Pending => {}
        }

        match this.sleep.poll(cx) {
            Poll::Ready(()) => {
                // Construct and return a timeout error.
                let error = Box::new(TimeoutError(()));
                return Poll::Ready(Err(error));
            }
            Poll::Pending => {}
        }

        Poll::Pending
    }
}
```

Finally we have to revisit our `Service` implementation and update it to also
use `BoxError`:

```rust
impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
    // Same trait bound like we had on `impl Future for ResponseFuture`.
    S::Error: Into<BoxError>,
{
    type Response = S::Response;
    // The error type of `Timeout` is now `BoxError`.
    type Error = BoxError;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Have to map the error type here as well.
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response_future = self.inner.call(request);
        let sleep = tokio::time::sleep(self.timeout);

        ResponseFuture {
            response_future,
            sleep,
        }
    }
}
```

## Conclusion

Thats it! We've now successfully implemented the `Timeout` middleware as it
exists in Tower today.

Our final implementation is:

```rust
use pin_project::pin_project;
use std::time::Duration;
use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::time::Sleep;
use tower::Service;

#[derive(Debug, Clone)]
struct Timeout<S> {
    inner: S,
    timeout: Duration,
}

impl<S> Timeout<S> {
    fn new(inner: S, timeout: Duration) -> Self {
        Timeout { inner, timeout }
    }
}

impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
    S::Error: Into<BoxError>,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response_future = self.inner.call(request);
        let sleep = tokio::time::sleep(self.timeout);

        ResponseFuture {
            response_future,
            sleep,
        }
    }
}

#[pin_project]
struct ResponseFuture<F> {
    #[pin]
    response_future: F,
    #[pin]
    sleep: Sleep,
}

impl<F, Response, Error> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response, Error>>,
    Error: Into<BoxError>,
{
    type Output = Result<Response, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.response_future.poll(cx) {
            Poll::Ready(result) => {
                let result = result.map_err(Into::into);
                return Poll::Ready(result);
            }
            Poll::Pending => {}
        }

        match this.sleep.poll(cx) {
            Poll::Ready(()) => {
                let error = Box::new(TimeoutError(()));
                return Poll::Ready(Err(error));
            }
            Poll::Pending => {}
        }

        Poll::Pending
    }
}

#[derive(Debug, Default)]
struct TimeoutError(());

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("request timed out")
    }
}

impl std::error::Error for TimeoutError {}

type BoxError = Box<dyn std::error::Error + Send + Sync>;
```

You can find the code in Tower [here][timeout-in-tower].

The pattern of implementing `Service` for some type that wraps another `Service`
and returning a `Future` that wraps another `Future` is how most Tower
middleware work.

Some other good examples are:

- [`ConcurrencyLimit`]: Limit the max number of requests being concurrently processed.
- [`LoadShed`]: For shedding load when inner services aren't ready.
- [`Steer`]: Routing between services.

With this you should be fully equipped to write robust production ready
middleware. If you want more practice here are some exercises to play with:

- Implement our timeout middleware but using [`tokio::time::timeout`] instead of
  `sleep`.
- Implement `Service` adapters similar to `Result::map` and `Result::map_err`
  that transforms the request, response, or error using a closure given by the
  user.
- Implement [`ConcurrencyLimit`]. Hint: You're going to need [`PollSemaphore`] to
  implement `poll_ready`.

If you have questions you're welcome to post in `#tower` in the [Tokio Discord
server][discord].

[^1]: The Rust compiler teams plans to add a feature called ["`impl Trait` in
type aliases"](https://github.com/rust-lang/rust/issues/63063) which would
allow us to return `impl Future` from `call` but for now it isn't possible.

[invent]: https://tokio.rs/blog/2021-05-14-inventing-the-service-trait
[`Service`]: https://docs.rs/tower/latest/tower/trait.Service.html
[`tower::timeout::Timeout`]: https://docs.rs/tower/latest/tower/timeout/struct.Timeout.html
[`Pin`]: https://doc.rust-lang.org/stable/std/pin/struct.Pin.html
[pin]: https://www.youtube.com/watch?v=DkMwYxfSYNQ
[pin-project]: https://crates.io/crates/pin-project
[timeout-in-tower]: https://github.com/tower-rs/tower/blob/master/tower/src/timeout/mod.rs
[`ConcurrencyLimit`]: https://github.com/tower-rs/tower/blob/master/tower/src/limit/concurrency/service.rs
[`LoadShed`]: https://github.com/tower-rs/tower/blob/master/tower/src/load_shed/mod.rs
[`Steer`]: https://github.com/tower-rs/tower/blob/master/tower/src/steer/mod.rs
[`tokio::time::timeout`]: https://docs.rs/tokio/latest/tokio/time/fn.timeout.html
[`tokio::time::sleep`]: https://docs.rs/tokio/latest/tokio/time/fn.sleep.html
[`PollSemaphore`]: https://docs.rs/tokio-util/latest/tokio_util/sync/struct.PollSemaphore.html
[discord]: https://discord.gg/tokio
[`Unpin`]: https://doc.rust-lang.org/stable/std/marker/trait.Unpin.html
[rust-guidelines]: https://rust-lang.github.io/api-guidelines/future-proofing.html#c-struct-bounds
