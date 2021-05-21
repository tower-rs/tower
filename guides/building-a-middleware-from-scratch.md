# Building a middleware from scratch

In ["Inventing the `Service` trait"][invent] we learned all the motivations
behind [`Service`] and why its designed the way it is. We also built a few
smaller middleware ourselves but we took a few shortcuts in our implementation.
In this guide we're going to build `Timeout` as it exists in Tower today without
taking any shortcuts.

Writing a robust middleware requires working with async Rust at a slightly lower
level than you might be used to. The goal of this guide is to demystify the
concepts and patterns so you can start writing your own middleware and maybe
even contribute back to the Tower ecosystem!

## Getting started

The middleware we're going to build is [`tower::timeout::Timeout`]. It will set
a limit on the maximum duration its inner `Service` is allowed to take. If it
doesn't produce a response within some amount of time, an error is returned.
This allows the client to retry that request or report an error to the user,
rather than waiting forever.

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
services to clone such that you can convert the `&mut self` given to
`Service::call` into an owned `self` that can be moved into the response future,
if necessary. We should therefore add `#[derive(Clone)]` to our struct. We
should also drive `Debug` while we're at it:

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

Note that we don't have to add any trait bounds on `S` even though we expect `S`
to implement `Service`. In Rust is common practice to only add trait bounds on
the places that actually need them and since `new` doesn't we leave them off.

Now the interesting bit. How to implement `Service` for `Timeout<S>`. Lets start
with an implementation that just forwards everything to the inner service:

```rust
use tower_service::Service;
use std::task::{Context, Poll};

impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
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

The approach we're going to take is to call `tokio::time::sleep` to get a future
that completes when we're out of them and then select the value from whichever
of the two futures is the first to complete.

Creating both futures is done like this:

```rust
use tokio::time::sleep;

fn call(&mut self, request: Request) -> Self::Future {
    let response = self.inner.call(request);

    // This variable has type `tokio::time::Sleep`.
    //
    // We don't have to clone `self.duration` as it implements the `Copy` trait.
    let sleep = tokio::time::sleep(self.timeout);

    // what to write here?
}
```

One possible return type is be `Pin<Box<dyn Future<...>>>`. However we want our
`Timeout` to add as little overhead as possible so we would like to find a way
to avoid allocating a `Box`. Imagine we have a large stack, with dozens of
nested `Service`s, where each layer allocates a new `Box` for every request, that
passes through it. That would result in a lot of allocations which might impact
performance.

To get around this we have to write our own future type. Lets create a struct
called `ResponseFuture`. It has to be generic over the inner service's response
future type. This is analogous to wrapping services in other services, but this
time we're wrapping futures in other futures.

```rust
use tokio::time::Sleep;

pub struct ResponseFuture<F> {
    response: F,
    sleep: Sleep,
}
```

`F` will be the type of `self.inner.call(request)`. Updating our `call` function
we get:

```rust
impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;

    // We have to `Timeout`'s response future as well.
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = self.inner.call(request);
        let sleep = tokio::time::sleep(self.timeout);

        // Create our response future with the future from the inner service
        ResponseFuture { response, sleep }
    }
}
```

A key point here is that Rust's futures are _lazy_. That means nothing actually
happens until they're `await`ed to polled. So `self.inner.call(request)` will
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

Ideally what we want to write inside `poll` is something like:

1. First poll `self.response` and if its done return the response or error it
   resolved to.
2. Otherwise poll `self.sleep` and if its done return an error.
3. Since neither future is ready return `Poll::Pending`.

We might try:

```rust
fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    match self.response.poll(cx) {
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
56 |         match self.response.poll(cx) {
   |                             ^^^^ method not found in `F`
   |
   = help: items from traits can only be used if the type parameter is bounded by the trait
help: the following traits define an item `poll`, perhaps you need to restrict type parameter `F` with one of them:
   |
49 | impl<F: Future, Response, Error> Future for ResponseFuture<F>
   |      ^^^^^^^^^

error: aborting due to previous error
```

Unfortunately the error we get from Rust isn't very good. It tells us to add a
`F: Future` bound even though we've already done that with `where F:
Future<Output = Result<Response, E>>`.

The real issue has to do with [`Pin`]. The full details of pinning is outside
the scope of this guide. If you're brand new to `Pin` we recommend ["The Why,
What, and How of Pinning in Rust"][pin] by Jon Gjengset.

What Rust is trying to tell us is that we need a `Pin<&mut F>` to be able to
call `poll`. Accessing `F` through `self.response` when `self` is a `Pin<&mut
Self>` doesn't work.

One solution is to require that `F` implements `Unpin`. `Unpin` basically means
that we can ignore `Pin` and treat a `Pin<&mut Self>` as just a `&mut self` as
if the `Pin` wasn't there. That looks like this:

```rust
impl<F, Response, Error> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response, Error>> + Unpin,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.response).poll(cx) {
            Poll::Ready(result) => return Poll::Ready(result),
            Poll::Pending => {}
        }

        todo!()
    }
}
```

[invent]: https://tokio.rs/blog/2021-05-14-inventing-the-service-trait
[`Service`]: https://docs.rs/tower/latest/tower/trait.Service.html
[`tower::timeout::Timeout`]: https://docs.rs/tower/latest/tower/timeout/struct.Timeout.html
[`Pin`]: https://doc.rust-lang.org/stable/std/pin/struct.Pin.html
[pin]: https://www.youtube.com/watch?v=DkMwYxfSYNQ
