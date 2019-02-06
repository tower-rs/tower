# Tower

Tower is a library of modular and reusable components for building robust
networking clients and servers.

[![Build Status](https://travis-ci.org/tower-rs/tower.svg?branch=master)](https://travis-ci.org/tower-rs/tower)
[![Gitter](https://badges.gitter.im/tower-rs/tower.svg)](https://gitter.im/tower-rs/tower)

## Overview

Tower aims to make it as easy as possible to build robust networking clients and
servers. It is protocol agnostic, but is designed around a request / response
pattern. If your protocol is entirely stream based, Tower may not be a good fit.

## Project Layout

Tower consists of a number of components, each of which live in their own sub
crates.

* [`tower-service`]: The foundational traits upon which Tower is built
  ([docs][ts-docs])

* [`tower-balance`]: A load balancer. Load is balanced across a number of
  services ([docs][tb-docs].

* [`tower-buffer`]: A buffering middleware. If the inner service is not ready to
  handle the next request, `tower-buffer` stores the request in an internal
  queue ([docs][tbuf-docs]).

* [`tower-discover`]: Service discovery abstraction ([docs][td-docs]).

* [`tower-filter`]: Middleware that conditionally dispatch requests to the inner
  service based on a predicate ([docs][tf-docs]);

* [`tower-in-flight-limit`]: Middleware limiting the number of requests that
  are in-flight for the inner service ([docs][tifl-docs]).

* [`tower-mock`]: Testing utility for mocking a `Service`. This is useful for
  testing middleware implementations ([docs][tm-docs]);

* [`tower-rate-limit`]: Middleware limiting the number of requests to the inner
  service over a period of time ([docs][trl-docs]).

* [`tower-reconnect`]: Middleware that automatically reconnects the inner
  service when it becomes degraded ([docs][tre-docs]).

* [`tower-retry`]: Middleware that retries requests based on a given `Policy`
  ([docs][tretry-docs]).

* [`tower-timeout`]: Middleware that applies a timeout to requests
  ([docs][tt-docs]).

* [`tower-util`]: Miscellaneous additional utilities for Tower
  ([docs][tu-docs]).

* [`tower-watch`]: A middleware that rebinds the inner service each time a watch
  is notified ([docs][tw-docs]).

## Status

Currently, only [`tower-service`], the foundational trait, has been released to
crates.io. The rest of the library will be following shortly.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tower by you, shall be licensed as MIT, without any additional
terms or conditions.

[`tower-service`]: tower-service
[ts-docs]: https://docs.rs/tower-service/
[`tower-balance`]: tower-balance
[tb-docs]: https://tower-rs.github.io/tower/tower_balance/index.html
[`tower-buffer`]: tower-buffer
[tbuf-docs]: https://tower-rs.github.io/tower/tower_buffer/index.html
[`tower-discover`]: tower-discover
[td-docs]: https://tower-rs.github.io/tower/tower_discover/index.html
[`tower-filter`]: tower-filter
[tf-docs]: https://tower-rs.github.io/tower/tower_filter/index.html
[`tower-in-flight-limit`]: tower-in-flight-limit
[tifl-docs]: https://tower-rs.github.io/tower/tower_in_flight_limit/index.html
[`tower-mock`]: tower-mock
[tm-docs]: https://tower-rs.github.io/tower/tower_mock/index.html
[`tower-rate-limit`]: tower-rate-limit
[trl-docs]: https://tower-rs.github.io/tower/tower_rate_limit/index.html
[`tower-reconnect`]: tower-reconnect
[tre-docs]: https://tower-rs.github.io/tower/tower_reconnect/index.html
[`tower-retry`]: tower-retry
[tretry-docs]: https://tower-rs.github.io/tower/tower_retry/index.html
[`tower-timeout`]: tower-timeout
[tt-docs]: https://tower-rs.github.io/tower/tower_timeout/index.html
[`tower-util`]: tower-util
[tu-docs]: https://tower-rs.github.io/tower/tower_util/index.html
[`tower-watch`]: tower-watch
[tw-docs]: https://tower-rs.github.io/tower/tower_watch/index.html
