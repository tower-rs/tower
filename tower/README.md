# Tower

Tower is a library of modular and reusable components for building robust
networking clients and servers.

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![Documentation (master)][docs-master-badge]][docs-master-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]
[![Discord chat][discord-badge]][discord-url]

[crates-badge]: https://img.shields.io/crates/v/tower.svg
[crates-url]: https://crates.io/crates/tower
[docs-badge]: https://docs.rs/tower/badge.svg
[docs-url]: https://docs.rs/tower
[docs-master-badge]: https://img.shields.io/badge/docs-master-blue
[docs-master-url]: https://tower-rs.github.io/tower/tower
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE
[actions-badge]: https://github.com/tower-rs/tower/workflows/CI/badge.svg
[actions-url]:https://github.com/tower-rs/tower/actions?query=workflow%3ACI
[discord-badge]: https://img.shields.io/discord/500028886025895936?logo=discord&label=discord&logoColor=white
[discord-url]: https://discord.gg/EeF3cQw
## Overview

Tower aims to make it as easy as possible to build robust networking clients and
servers. It is protocol agnostic, but is designed around a request / response
pattern. If your protocol is entirely stream based, Tower may not be a good fit.

Tower provides a simple core abstraction, the [`Service`] trait, which
represents an asynchronous function taking a request and returning either a
response or an error. This abstraction can be used to model both clients and
servers.

Generic components, like [timeouts], [rate limiting], and [load balancing],
can be modeled as [`Service`]s that wrap some inner service and apply
additional behavior before or after the inner service is called. This allows
implementing these components in a protocol-agnostic, composable way. Typically,
such services are referred to as _middleware_.

An additional abstraction, the [`Layer`] trait, is used to compose
middleware with [`Service`]s. If a [`Service`] can be thought of as an
asynchronous function from a request type to a response type, a [`Layer`] is
a function taking a [`Service`] of one type and returning a [`Service`] of a
different type. The [`ServiceBuilder`] type is used to add middleware to a
service by composing it with multiple multiple [`Layer`]s.

### The Tower Ecosystem

Tower is made up of the following crates:

* [`tower`] (this crate)
* [`tower-service`]
* [`tower-layer`]
* [`tower-test`]

Since the [`Service`] and [`Layer`] traits are important integration points
for all libraries using Tower, they are kept as stable as possible, and
breaking changes are made rarely. Therefore, they are defined in separate
crates, [`tower-service`] and [`tower-layer`]. This crate contains
re-exports of those core traits, implementations of commonly-used
middleware, and [utilities] for working with [`Service`]s and [`Layer`]s.
Finally, the [`tower-test`] crate provides tools for testing programs using
Tower.

## Usage

The various middleware implementations provided by this crate are feature
flagged, so that users can only compile the parts of Tower they need. By
default, all the optional middleware are disabled.

To get started using all of Tower's optional middleware, add this to your
`Cargo.toml`:

```toml
tower = { version = "0.4", features = ["full"] }
```

Alternatively, you can only enable some features. For example, to enable
only the [`retry`] and [`timeout`][timeouts] middleware, write:

```toml
tower = { version = "0.4", features = ["retry", "timeout"] }
```

See [here](modules) for a complete list of all middleware provided by
Tower.

[`Service`]: https://docs.rs/tower/latest/tower/trait.Service.html
[`Layer]: https://docs.rs/tower/latest/tower/trait.Layer.html
[timeouts]: https://docs.rs/tower/latest/tower/timeout/
[rate limiting]: https://docs.rs/tower/latest/tower/limit/rate
[load balancing]: https://docs.rs/tower/latest/tower/balance/
[`ServiceBuilder`]: https://docs.rs/tower/latest/tower/struct.ServiceBuilder.html
[utilities]: https://docs.rs/tower/latest/tower/trait.ServiceExt.html
[`tower`]: https://crates.io/crates/tower
[`tower-service`]: https://crates.io/crates/tower-service
[`tower-layer`]: https://crates.io/crates/tower-layer
[`tower-test`]: https://crates.io/crates/tower-test
[`retry`]: https://docs.rs/tower/latest/tower/retry

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tower by you, shall be licensed as MIT, without any additional
terms or conditions.
