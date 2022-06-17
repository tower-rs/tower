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

## Supported Rust Versions

Tower will keep a rolling MSRV (minimum supported Rust version) policy of **at
least** 6 months. When increasing the MSRV, the new Rust version must have been
released at least six months ago. The current MSRV is 1.49.0.

## Getting Started

If you're brand new to Tower and want to start with the basics we recommend you
check out some of our [guides].

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tower by you, shall be licensed as MIT, without any additional
terms or conditions.

[guides]: https://github.com/tower-rs/tower/tree/master/guides
