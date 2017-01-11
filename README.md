# tokio-service

Definition of the core `Service` trait in Tokio

[![Build Status](https://travis-ci.org/tokio-rs/tokio-service.svg?branch=master)](https://travis-ci.org/tokio-rs/tokio-service)
[![Build status](https://ci.appveyor.com/api/projects/status/qgostmtadmlqae8n?svg=true)](https://ci.appveyor.com/project/alexcrichton/tokio-service)

[Documentation](https://docs.rs/tokio-service)

## Usage

First, add this to your `Cargo.toml`:

```toml
[dependencies]
tokio-service = { git = "https://github.com/tokio-rs/tokio-service" }
```

Next, add this to your crate:

```rust
extern crate tokio_service;
```

You can find extensive examples and tutorials in addition to the [API
documentation](https://docs.rs/tokio-service) at
[https://tokio.rs](https://tokio.rs)

# License

`tokio-service` is primarily distributed under the terms of both the MIT
license and the Apache License (Version 2.0), with portions covered by various
BSD-like licenses.

See LICENSE-APACHE, and LICENSE-MIT for details.
