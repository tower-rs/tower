# Tower Guides

These guides are meant to be an introduction to Tower. At least basic Rust
experience is assumed. Some experience with asynchronous Rust is also
recommended. If you're brand new to async Rust, we recommend the [Asynchronous
Programming in Rust][async-book] book or the [Tokio tutorial][tokio-tutorial].

Additionally, some of these guides explain Tower from the perspective of HTTP
servers and clients. However, Tower is useful for any network protocol that
follows an async request/response pattern. HTTP is used here because it is a
widely known protocol, and one of Tower's more common use-cases.

## Guides

- ["Inventing the `Service` trait"][invent] walks through how Tower's
  fundamental [`Service`] trait could be designed from scratch. If you have no
  experience with Tower and want to learn the absolute basics, this is where you
  should start.
- ["Building a middleware from scratch"][build] walks through how to build the
  [`Timeout`] middleware as it exists in Tower today, without taking any shortcuts.

[async-book]: https://rust-lang.github.io/async-book/
[tokio-tutorial]: https://tokio.rs/tokio/tutorial
[invent]: https://tokio.rs/blog/2021-05-14-inventing-the-service-trait
[build]: https://github.com/tower-rs/tower/blob/master/guides/building-a-middleware-from-scratch.md
[`Service`]: https://docs.rs/tower/latest/tower/trait.Service.html
[`Timeout`]: https://docs.rs/tower/latest/tower/timeout/struct.Timeout.html
