# Inventing the `Service` trait

At the core of Tower is the `Service` trait. It models an asynchronous function
that takes a request and produces a response. However rather than diving into
exactly what that means right away lets instead see how you could have invented
the `Service` trait yourself, from scratch.

Imagine you were building a little HTTP framework in Rust. Something that allows
users to easily receive requests and reply with some response.

You might have an API like this:

```rust
// Create a server that listens on port 3000
let server = Server::new("127.0.0.1:3000").await?;

// Somehow run the user's application
server.run(the_users_application).await?;
```

The question is, what should `the_users_application` be?

The simplest thing that just might work is:

```rust
fn handle_request(request: Request) -> Response {
    // ...
}
```

Where `Request` and `Response` are some structs provided by our framework.

With this we can implement `Server::run` like so:

```rust
impl Server {
    async fn run<F>(self, handler: F) -> Result<(), Error>
    where
        F: Fn(Request) -> Response,
    {
        loop {
            let mut connection = receive_connection().await?;
            let request = parse_http_request(&mut connection).await?;

            // Call the handler provided by the user
            let response = handler(request);

            write_http_response(connection).await?;
        }
    }
}
```

Here we have an asynchronous function `run` which takes a closure that accepts a
`Request` and returns a `Response`.

That means users can use our `Server` like so:

```rust
fn handle_request(request: Request) -> Response {
    if request.path() == "/" {
        Response::ok("Hello, World!")
    } else {
        Response::not_found()
    }
}

// Run the server and handle requests using our `handle_request` function
server.run(handle_request).await?;
```

This is not bad. It makes it easy for users to run HTTP servers without worrying
about any of the low level details.

However our current design has one issue: We cannot handle the request
asynchronously. Imagine our user needs to query a database or send a request to
some other server while handling the request. That would currently require
blocking while we wait for a response. That isn't great. Lets fix that by having
the handler function return a future:

```rust
impl Server {
    async fn run<F, Fut>(self, handler: F) -> Result<(), Error>
    where
        // `handler` now returns a generic type `Fut`...
        F: Fn(Request) -> Fut,
        // ...which is a `Future` who's `Output` is a `Response`
        Fut: Future<Output = Response>,
    {
        loop {
            let mut connection = receive_connection().await?;
            let request = parse_http_request(&mut connection).await?;
            // Await the future returned by `handler`
            let response = handler(request).await;
            write_http_response(connection).await?;
        }
    }
}
```

Using this API is very similar to before:

```rust
// Now an async function
async fn handle_request(request: Request) -> Response {
    if request.path() == "/" {
        Response::ok("Hello, World!")
    } else if request.path() == "/important-data" {
        // We can now do async stuff in here
        let some_data = fetch_data_from_database().await;
        make_response(some_data)
    } else {
        Response::not_found()
    }
}

// Running the server is the same
server.run(handle_request).await?;
```

This is much nicer since our request handling can now call other async
functions. However there is still something missing. What if our handler
encounters an error and cannot produce a response? Lets make it return a
`Result`:

```rust
impl Server {
    async fn run<F, Fut>(self, handler: F) -> Result<(), Error>
    where
        F: Fn(Request) -> Fut,
        // The response future is now allowed to fail
        Fut: Future<Output = Result<Response, Error>>,
    {
        loop {
            let mut connection = receive_connection().await?;
            let request = parse_http_request(&mut connection).await?;
            match handler(request).await {
                Ok(response) => write_http_response(connection).await?,
                Err(error) => handle_error_somehow(error),
            }
        }
    }
}
```

## Adding on more behavior

Next-up you might want to add supports for timeouts. A timeout is the max
duration `handler` is allowed to take. If it doesn't produce a response within
that amount of time an error is returned.

Your first idea might be to modify `Server` to support being configured with a
timeout which it will somehow apply to the call of the handler. However it turns out
you can actually add a timeout without modifying `Server`. Using
`tokio::time::timeout` we can make a new handler function that calls our
previous `handle_request` but with a timeout of 30 seconds:

```rust
async fn handler_with_timout(request: Request) -> Result<Response, Error> {
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        handle_request(request)
    ).await;

    match result {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(error)) => Err(error),
        Err(_timeout_elapsed) => Err(Error::timeout()),
    }
}
```

This provides a quite nice separation of concerns. We were able to add a timeout
without changing any of our existing code.

Lets add one more feature in this way. Imagine we're building a JSON API and
would therefore like a `Content-Type: application/json` header on all responses.
We can wrap `handler_with_timeout` in a similar way and modify the response like
so:

```rust
async fn handler_with_timout_and_content_type(
    request: Request,
) -> Result<Response, Error> {
    let mut response = handler_with_timout(request).await?;
    response.set_header("Content-Type", "application/json");
    Ok(response)
}
```

We now have a handler that will process an HTTP request, take no longer than 30
seconds, and always have the right `Content-Type` header all without modifying
our original `handle_request` function or `Server` struct.

Designing libraries that can be extended in this way is very powerful since it
allows users to extend things without having to wait for the library maintainers
to add support for it.

However there is one problem. Our current setup doesn't scale very well. Imagine
we have many `handle_with_*` functions that each add a little bit of behavior.
Having to hard code the chain of which intermediate handler calls which isn't
great. Our current chain is

1. `handler_with_timout_and_content_type` which calls
2. `handler_with_timout` which calls
3. `handle_request` which actually processes the request

It would be nice if we could somehow compose these three functions without
having to hard code the exact order. Something like:

```rust
let final_handler = with_content_type(with_timeout(handle_request));
```

While still being able to run our handler like before:

```rust
server.run(final_handler).await?;
```

You could imagine `with_content_type` and `with_timeout` being functions that
took an argument of type `F: Fn(Request) -> Future<Output = Response>` and
returned a closure a la `impl Fn(Request) -> Future<Output = Result<Response,
Error>>` but all these closure types quickly become hard to deal with.

## The `Handler` trait

Lets try another approach. Rather than `Server::run` accepting a closure
(`Fn(Request) -> ...`) lets make a new trait that encapsulates the same `async
fn(Request) -> Result<Response, Error>` setup:

```rust
trait Handler {
    async fn call(&mut self, request: Request) -> Result<Response, Error>;
}
```

Having a trait like this allows us to write concrete types that implement it so
we don't have to deal with `Fn`s all the time.

However Rust currently doesn't support async traits so we have two options:

1. Make `call` return a boxed future like `Pin<Box<dyn Future<Output =
   Result<Response, Error>>>`. This is what the [async-trait] crate does.
2. Add an associated `type Future` to `Handler` so users get to pick their own
   type.

Lets go with option 2 as its the most flexible. Users who have a concrete future
type can use that without the cost of a `Box` and users who don't care can still
use `Pin<Box<...>>`.

```rust
trait Handler {
    type Future: Future<Output = Result<Response, Error>>;

    fn call(&mut self, request: Request) -> Self::Future;
}
```

We still have to require that `Handler::Future` implements `Future` where the
output type is `Result<Response, Error>` as that is what `Server::run` requires.

Lets convert our original `handle_request` function into an implementation of
this trait:

```rust
struct RequestHandler;

impl Handler for RequestHandler {
    // We use `Pin<Box<...>>` here for simplicity
    // but something like `futures::future::Ready` would also work
    type Future = Pin<Box<dyn Future<Output = Result<Response, Error>>>>;

    fn call(&mut self, request: Request) -> Self::Future {
        Box::pin(async move {
            // same implementation as we had before
            if request.path() == "/" {
                Ok(Response::ok("Hello, World!"))
            } else if request.path() == "/important-data" {
                let some_data = fetch_data_from_database().await?;
                Ok(make_response(some_data))
            } else {
                Ok(Response::not_found())
            }
        })
    }
}
```

How about supporting timeouts? Remember, the solution we're aiming for is one
that allows us to combine different pieces of functionality together without
having to modify each individual piece.

What if we defined a generic `Timeout` struct like this:

```rust
struct Timeout<T> {
    // T will be some type that implements `Handler`
    inner_handler: T,
    duration: Duration,
}
```

We could then implement `Handler` for `Timeout<T>` and delegate to `T`'s
`Handler` implementation:

```rust
impl<T> Handler for Timeout<T>
where
    T: Handler,
{
    type Future = Pin<Box<dyn Future<Output = Result<Response, Error>>>>;

    fn call(&mut self, request: Request) -> Self::Future {
        Box::pin(async move {
            let result = tokio::time::timeout(
                self.duration,
                self.inner_handler.call(request),
            ).await;

            match result {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(error)) => Err(error),
                Err(_timeout) => Err(Error::timeout()),
            }
        })
    }
}
```

The important line being `self.inner_handler.call(request)`. This is where we
delegate to the inner handler and let it do its thing. We don't know what it is
we just know that it produces a `Result<Response, Error>` when its done.

This code doesn't quite compile though. We get an error like this:

```
error[E0759]: `self` has an anonymous lifetime `'_` but it needs to satisfy a `'static` lifetime requirement
   --> src/lib.rs:145:29
    |
144 |       fn call(&mut self, request: Request) -> Self::Future {
    |               --------- this data with an anonymous lifetime `'_`...
145 |           Box::pin(async move {
    |  _____________________________^
146 | |             let result = tokio::time::timeout(
147 | |                 self.duration,
148 | |                 self.inner_handler.call(request),
...   |
155 | |             }
156 | |         })
    | |_________^ ...is captured here, requiring it to live as long as `'static`
```

The issue is that we're capturing a `&mut self` and moving it into an async
block. That means the lifetime of our future is tied to the lifetime of `&mut
self`. This doesn't work for us since we might want to run our response futures
on multiple threads to get better performance or produce multiple response
futures and run them all in parallel. That isn't possible if a reference to the
handler lives inside the futures.

Instead we need convert the `&mut self` into an owned `self`. That is exactly
what `Clone` does:

```rust
// this must be `Clone` for `Timeout<T>` to be `Clone`
#[derive(Clone)]
struct RequestHandler;

impl Handler for RequestHandler {
    // ...
}

#[derive(Clone)]
struct Timeout<T> {
    inner_handler: T,
    duration: Duration,
}

impl<T> Handler for Timeout<T>
where
    T: Handler + Clone,
{
    type Future = Pin<Box<dyn Future<Output = Result<Response, Error>>>>;

    fn call(&mut self, request: Request) -> Self::Future {
        // get an owned clone of `&mut self`
        let mut this = self.clone();

        Box::pin(async move {
            let result = tokio::time::timeout(
                this.duration,
                this.inner_handler.call(request),
            ).await;

            match result {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(error)) => Err(error),
                Err(_timeout) => Err(Error::timeout()),
            }
        })
    }
}
```

Note that cloning is very cheap in this case since `RequestHandler` doesn't have
any data and `Timeout<T>` only adds a `Duration`.

One step closer. We now get a different error:

```
error[E0310]: the parameter type `T` may not live long enough
   --> src/lib.rs:149:9
    |
140 |   impl<T> Handler for Timeout<T>
    |        - help: consider adding an explicit lifetime bound...: `T: 'static`
...
149 | /         Box::pin(async move {
150 | |             let result = tokio::time::timeout(
151 | |                 this.duration,
152 | |                 this.inner_handler.call(request),
...   |
159 | |             }
160 | |         })
    | |__________^ ...so that the type `impl Future` will meet its required lifetime bounds
```

The problem now is that `T` can be any type whatsoever. It can even be a type
that contains references like `Vec<&'a str>`. However that wont work for the
same reason as before. We need the response future to have a `'static` lifetime
so we can more easily pass it around.

The compiler actually tells us what the fix is. Add `T: 'static`:

```rust
impl<T> Handler for Timeout<T>
where
    T: Handler + Clone + 'static,
{
    // ...
}
```

It now compiles!

Lets create a similar handler struct for adding a `Content-Type` header on the
response:

```rust
#[derive(Clone)]
struct JsonContentType<T> {
    inner_handler: T,
}

impl<T> Handler for JsonContentType<T>
where
    T: Handler + Clone + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Result<Response, Error>>>>;

    fn call(&mut self, request: Request) -> Self::Future {
        let mut this = self.clone();

        Box::pin(async move {
            let mut response = this.inner_handler.call(request).await?;
            response.set_header("Content-Type", "application/json");
            Ok(response)
        })
    }
}
```

Notice this follows a very similar pattern to `Timeout`.

Next up we modify `Server::run` to accept our new `Handler` trait:

```rust
impl Server {
    async fn run<T>(self, mut handler: T) -> Result<(), Error>
    where
        T: Handler,
    {
        loop {
            let mut connection = receive_connection().await?;
            let request = parse_http_request(&mut connection).await?;
            // have to call `Handler::call` here
            match handler.call(request).await {
                Ok(response) => write_http_response(connection).await?,
                Err(error) => handle_error_somehow(error),
            }
        }
    }
}
```

We can now compose our three handlers together and run our server like so:

```rust
JsonContentType {
    inner_handler: Timeout {
        inner_handler: RequestHandler,
        duration: Duration::from_secs(30),
    },
}
```

And if we add some `new` methods to our types they become a little easier to
compose:

```rust
let handler = RequestHandler;
let handler = Timeout::new(handler, Duration::from_secs(30));
let handler = JsonContentType::new(handler);

// `handler` has type `JsonContentType<Timeout<RequestHandler>>`

server.run(handler).await
```

This works quite well! We're now able to layer on additional functionality to
`RequestHandler` without having to modify its implementation. In theory we could
put our `JsonContentType` and `Timeout` handlers into a crate and release it as
library on crates.io for other people to use!

## Making `Handler` more flexible

Our little `Handler` trait is working quite nicely but currently it only
supports our `Request` and `Response` types. It would be nice if those where
generic so users could use whatever type they want:

```rust
// Lets make the request type a generic type parameter.
// This allows users to implement `Handler` multiple times for
// the same type. Its not something we need but our users might.
trait Handler<Request> {
    // Similarly to `Future` we add an associated type for the
    // response
    type Response;

    // Might as well add one for error as well. No reason for that
    // to be a hardcoded type
    type Error;

    // Our future type from before but now its output must use
    // the generic `Response` and `Error` types
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    // `call` is unchanged but note that `Request` here is our generic
    // `Request` type paramter and not the `Request` type we've used
    // until now
    fn call(&mut self, request: Request) -> Self::Future;
}
```

Our implementation for `RequestHandler` now becomes

```rust
impl Handler<Request> for RequestHandler {
    type Response = Response;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Error>>>>;

    fn call(&mut self, request: Request) -> Self::Future {
        // same as before
    }
}
```

`Timeout<T>` is a bit different. Since it wraps some other `Handler` and adds an
async timeout it actually doesn't care about what the request or response types
are. As long as the `Handler` it is wrapping uses the same types.

The `Error` type is a bit different. Since `tokio::time::timeout` returns
`Result<T, tokio::time::error::Elapsed>` we must be able to convert a
`tokio::time::error::Elapsed` into the inner `Handler`'s error type.

If we put all those things together we get

```rust
// `Timeout` accepts any request of type `R` as long as `T`
// accepts the same type of request
impl<R, T> Handler<R> for Timeout<T>
where
    // The actual type of request must not contain
    // references. The compiler would tell us to add
    // this if we didn't
    R: 'static,
    // `T` must accept requests of type `R`
    T: Handler<R> + Clone + 'static,
    // We must be able to convert an `Elapsed` into
    // `T`'s error type
    T::Error: From<tokio::time::error::Elapsed>,
{
    // Our response type is the same as `T`'s since we
    // don't have to modify the result
    type Response = T::Response;

    // Error type is also the same
    type Error = T::Error;

    // Future must output a `Result` with the new
    // response and error types
    type Future = Pin<Box<dyn Future<Output = Result<T::Response, T::Error>>>>;

    fn call(&mut self, request: R) -> Self::Future {
        let mut this = self.clone();

        Box::pin(async move {
            let result = tokio::time::timeout(
                this.duration,
                this.inner_handler.call(request),
            ).await;

            match result {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(error)) => Err(error),
                Err(elapsed) => {
                    // Convert the error
                    Err(T::Error::from(elapsed))
                }
            }
        })
    }
}
```

`JsonContentType` is a bit different. It doesn't care about the request or error
types but it does care about the response type. It must be `Response` such that
we can call `set_header`.

The implementation therefore is:

```rust
// Again a generic request type
impl<R, T> Handler<R> for JsonContentType<T>
where
    R: 'static,
    // `T` must accept requests of any type `R` and return
    // responses of type `Response`.
    T: Handler<R, Response = Response> + Clone + 'static,
{
    type Response = Response;

    // Our error type is whatever `T`'s error type is
    type Error = T::Error;

    type Future = Pin<Box<dyn Future<Output = Result<Response, T::Error>>>>;

    fn call(&mut self, request: R) -> Self::Future {
        let mut this = self.clone();

        Box::pin(async move {
            let mut response = this.inner_handler.call(request).await?;
            response.set_header("Content-Type", "application/json");
            Ok(response)
        })
    }
}
```

Finally the `Handler` passed to `Server::run` must use `Request` and `Response`:

```rust
impl Server {
    async fn run<T>(self, mut handler: T) -> Result<(), Error>
    where
        T: Handler<Request, Response = Response>,
    {
        // ...
    }
}
```

Creating the server has not changed:

```rust
let handler = RequestHandler;
let handler = Timeout::new(handler, Duration::from_secs(30));
let handler = JsonContentType::new(handler);

server.run(handler).await
```

So, we now have a `Handler` trait that makes it possible to break our
application up into small independent parts and re-use them. Not bad!

## "What if I told you..."

Until now we've only talked about HTTP from the servers perspective. But our
`Handler` trait actually fits HTTP clients as well. One can imagine a client
`Handler` that accepts some `Request` struct and asynchronously sends it to
someone on the internet. Our `Timeout` wrapper is actually useful here as well.
`JsonContentType` probably isn't since its not the clients job to set response
headers.

Since our `Handler` trait is useful for defining both servers and clients,
`Handler` probably isn't an appropriate name anymore. A client doesn't handle an
request, it sends it to a server and the server handles it. Lets instead call
our trait `Service`:

```rust
trait Service<Request> {
    type Response;
    type Error;
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    fn call(&mut self, request: Request) -> Self::Future;
}
```

This is actually _almost_ the `Service` trait defined in Tower. If you've been
able to follow along until this point you now understand most of Tower. Besides
the `Service` trait Tower also provides several utilities that implement
`Service` by wrapping some other type that also implements `Service`. Exactly
like we did with `Timeout` and `JsonContentType`. Things can be composed in ways
similar to what we've just did.

Some example services provided by Tower:

- `Timeout` - This is pretty much identical to the timeout we have built.
- `Retry` - To automatically retry failed requests.
- `RateLimit` - To limit the number of requests a service can handle over a
  period of time.

Types like `Timeout` and `JsonContentType` are typically called "middlewares"
since they wrap another `Service` and transform the request or response in some
way. Whereas types like `RequestHandler` is typically called "leaf services"
since it sits at the leaves of a tree of nested services. The actual responses
are normally produced in leaf services and modified by middlewares.

The only thing left to talk about is "backpressure" and `poll_ready`.

## Backpressure

Imagine you wanted to write a rate limit middleware that wraps a `Service` and
puts a limit on the maximum number of concurrent requests the underlying service
can will receive. This would be useful if you had some service that had a hard
upper limit on the amount of load it could handle.

With our current `Service` trait we don't really have a good way to implement
something like that. We could try:

```rust
impl<R, T> Service<R> for ConcurrencyLimit<T> {
    fn call(&mut self, request: R) -> Self::Future {
        // 1. Check a counter for the number of running requests
        //    currently.
        // 2. If there is capacity left send the request to `T`
        //    and increment the counter.
        // 3. If not somehow wait a bit and check the capacity
        //    again.
    }
}
```

If there is no capacity left we just have to wait a bit and then check again. So
if the service is receiving a lot of requests you can imagine waiting for a long
time.  It would also be nice if we had a way to tell the caller that there was
no more capacity left. Maybe they wanted to react to that by calling another
service instead.

In a way it would be nice if `Service` had a method like this:

```rust
trait Service<R> {
    async fn ready(&mut self);
}
```

`ready` would be an async function that completes when the service has capacity
enough to receive one new request. We would then require users to first call
`service.ready().await` before doing `service.call(request).await`.

This way way `ConcurrencyLimit` could track capacity inside `ready` and not
allow users to call `call` until there is sufficient capacity.

`Service`s that don't care about capacity can just return immediately from
`ready` or delegate to some inner `Service` they're wrapping.

However we still cannot define async functions inside traits. We could add
another associated type to `Service` called `ReadyFuture` but having to return a
`Future` will give us the same lifetime issues we've run into before. It would
be nice if there was someway around that.

Instead we can take some inspiration from the `Future` trait and define a method
called `poll_ready`:

```rust
use std::task::{Context, Poll};

trait Service<R> {
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<()>;
}
```

This means if the service is at capacity `poll_ready` will return
`Poll::Pending` and somehow ensure to call the provided `Context` when capacity
becomes available. At that point `poll_ready` can be called again and if it
returns `Poll::Ready(())` then capacity would be reserved and `call` can be
called.

It would still be possible get a `Future` that resolves when capacity is
avaiable using something like [`futures::future::poll_fn`] (or
[`tower::ServiceExt::ready`]).

This concept of services communicating with their callers about their capacity
is called "backpressure propagation". The fundamental idea is that you shouldn't
send a request to a service that doesn't have the capacity to handle it. Instead
you should wait, call another service, or somehow scale up the number of services
available.

Finally, it might also be possible for some error to happen while reserving
capacity so `poll_ready` probably should return `Poll<Result<(), Self::Error>>`.
With this change we've now arrived at the complete `tower::Service` trait:

```rust
pub trait Service<Request> {
    type Response;
    type Error;
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>>;

    fn call(&mut self, req: Request) -> Self::Future;
}
```

Middlewares that care about backpressure is pretty rare but it does enable some
interesting use cases such as various kinds of rate limiting, load balancing,
and auto scaling.

Since you never know exactly which middlewares a `Service` might consist of it
is important to not forget about `poll_ready`.

With all this in place, the most common way to call a service is:

```rust
use tower::{
    Service,
    // for the `ready` method
    ServiceExt,
};

let response = service
    // wait for the service to have capacity
    .ready().await?
    // send the request
    .call(request).await?;
```

[async-trait]: https://crates.io/crates/async-trait
[dropping]: https://doc.rust-lang.org/stable/std/ops/trait.Drop.html
[`futures::future::poll_fn`]: https://docs.rs/futures/0.3.14/futures/future/fn.poll_fn.html
[`tower::ServiceExt::ready`]: https://docs.rs/tower/0.4.7/tower/trait.ServiceExt.html#method.ready
