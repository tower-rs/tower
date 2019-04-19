# Tower

Tower is a library of modular and reusable components for building robust
networking clients and servers.

[![Build Status][azure-badge]][azure-url]
[![Gitter][gitter-badge]][gitter-url]

[azure-badge]: https://dev.azure.com/tower-rs/Tower/_apis/build/status/tower-rs.tower?branchName=master
[azure-url]: https://dev.azure.com/tower-rs/Tower/_build/latest?definitionId=1&branchName=master
[gitter-badge]: https://badges.gitter.im/tower-rs/tower.svg
[gitter-url]: https://gitter.im/tower-rs/tower

## Overview

Tower aims to make it as easy as possible to build robust networking clients and
servers. It is protocol agnostic, but is designed around a request / response
pattern. If your protocol is entirely stream based, Tower may not be a good fit.

## Project Layout

Tower consists of a number of components, each of which live in their own sub
crates.

* [`tower`]: The main user facing crate that provides batteries included tower services ([docs][t-docs]).

* [`tower-service`]: The foundational traits upon which Tower is built
  ([docs][ts-docs]).

* [`tower-layer`]: The foundational trait to compose services together
  ([docs][tl-docs]).

* [`tower-balance`]: A load balancer. Load is balanced across a number of
  services ([docs][tb-docs]).

* [`tower-buffer`]: A buffering middleware. If the inner service is not ready to
  handle the next request, `tower-buffer` stores the request in an internal
  queue ([docs][tbuf-docs]).

* [`tower-discover`]: Service discovery abstraction ([docs][td-docs]).

* [`tower-filter`]: Middleware that conditionally dispatch requests to the inner
  service based on a predicate ([docs][tf-docs]).

* [`tower-limit`]: Middleware limiting the number of requests that are
  processed ([docs][tlim-docs]).

* [`tower-reconnect`]: Middleware that automatically reconnects the inner
  service when it becomes degraded ([docs][tre-docs]).

* [`tower-retry`]: Middleware that retries requests based on a given `Policy`
  ([docs][tretry-docs]).

* [`tower-test`]: Testing utilies ([docs][ttst-docs]).

* [`tower-timeout`]: Middleware that applies a timeout to requests
  ([docs][tt-docs]).

* [`tower-util`]: Miscellaneous additional utilities for Tower
  ([docs][tu-docs]).

## Status

Currently, only [`tower-service`], the foundational trait, has been released to
crates.io. The rest of the library will be following shortly.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tower by you, shall be licensed as MIT, without any additional
terms or conditions.

[`tower`]: tower
[t-docs]: https://tower-rs.github.io/tower/doc/tower/index.html
[`tower-service`]: tower-service
[ts-docs]: https://docs.rs/tower-service/
[`tower-layer`]: tower-layer
[tl-docs]: https://docs.rs/tower-layer/
[`tower-balance`]: tower-balance
[tb-docs]: https://tower-rs.github.io/tower/doc/tower_balance/index.html
[`tower-buffer`]: tower-buffer
[tbuf-docs]: https://tower-rs.github.io/tower/doc/tower_buffer/index.html
[`tower-discover`]: tower-discover
[td-docs]: https://tower-rs.github.io/tower/doc/tower_discover/index.html
[`tower-filter`]: tower-filter
[tf-docs]: https://tower-rs.github.io/tower/doc/tower_filter/index.html
[`tower-limit`]: tower-limit
[tlim-docs]: https://tower-rs.github.io/tower/doc/tower_limit/index.html
[`tower-reconnect`]: tower-reconnect
[tre-docs]: https://tower-rs.github.io/tower/doc/tower_reconnect/index.html
[`tower-retry`]: tower-retry
[tretry-docs]: https://tower-rs.github.io/tower/doc/tower_retry/index.html
[`tower-timeout`]: tower-timeout
[`tower-test`]: tower-test
[ttst-docs]: https://tower-rs.github.io/tower/doc/tower_test/index.html
[`tower-rate-limit`]: tower-rate-limit
[tt-docs]: https://tower-rs.github.io/tower/doc/tower_timeout/index.html
[`tower-util`]: tower-util
[tu-docs]: https://tower-rs.github.io/tower/doc/tower_util/index.html
