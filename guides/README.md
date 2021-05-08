# Tower guides

These guides are meant to be an introduction to Tower. At least basic Rust
experience is assumed. Some experience with asynchronous Rust is also
recommended. If you're brand new to async Rust we recommend the [Asynchronous
Programming in Rust][async-book] book or the [Tokio tutorial][tokio-tutorial].

Additionally, some of these guides explain Tower from the perspective of HTTP
servers and clients. However Tower is useful for anything that follows and async
request/response pattern. Not just web services even though that is its primary
use case.

## Guides

- ["Inventing the `Service` trait"][invent] goes through how you could have
  invented Tower's fundamental `Service` trait starting from scratch. If you
  have no experience with Tower and want to learn the absolute basics this is
  where you should start.

[async-book]: https://rust-lang.github.io/async-book/
[tokio-tutorial]: https://tokio.rs/tokio/tutorial
[invent]: https://github.com/tower-rs/tower/blob/master/guides/inventing-service.md
