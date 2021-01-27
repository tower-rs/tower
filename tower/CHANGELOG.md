# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

# Unreleased

- **util**: Add `ServiceExt::map_future`. ([#542])

[#542]: https://github.com/tower-rs/tower/pull/542

# 0.4.4 (January 20, 2021)

### Added

- **util**: Implement `Layer` for `Either<A, B>`. ([#531])
- **util**: Implement `Clone` for `FilterLayer`. ([#535])
- **timeout**: Implement `Clone` for `TimeoutLayer`. ([#535])
- **limit**: Implement `Clone` for `RateLimitLayer`. ([#535])

### Fixed

- Added "full" feature which turns on all other features. ([#532])
- **spawn-ready**: Avoid oneshot allocations. ([#538])

[#531]: https://github.com/tower-rs/tower/pull/531
[#532]: https://github.com/tower-rs/tower/pull/532
[#535]: https://github.com/tower-rs/tower/pull/535
[#538]: https://github.com/tower-rs/tower/pull/538

# 0.4.3 (January 13, 2021)

### Added

- **filter**: `Filter::check` and `AsyncFilter::check` methods which check a
  request against the filter's `Predicate` ([#521])
- **filter**: Added `get_ref`, `get_mut`, and `into_inner` methods to `Filter`
  and `AsyncFilter`, allowing access to the wrapped service ([#522])
- **util**: Added `layer` associated function to `AndThen`, `Then`,
  `MapRequest`, `MapResponse`, and `MapResult` types. These return a `Layer`
  that produces middleware of that type, as a convenience to avoid having to
  import the `Layer` type separately. ([#524])
- **util**: Added missing `Clone` impls to `AndThenLayer`, `MapRequestLayer`,
  and `MapErrLayer`, when the mapped function implements `Clone` ([#525])
- **util**: Added `FutureService::new` constructor, with less restrictive bounds
  than the `future_service` free function ([#523])

[#521]: https://github.com/tower-rs/tower/pull/521
[#522]: https://github.com/tower-rs/tower/pull/522
[#523]: https://github.com/tower-rs/tower/pull/523
[#524]: https://github.com/tower-rs/tower/pull/524
[#525]: https://github.com/tower-rs/tower/pull/525

# 0.4.2 (January 11, 2021)

### Added

- Export `layer_fn` and `LayerFn` from the `tower::layer` module. ([#516])

### Fixed

- Fix missing `Sync` implementation for `Buffer` and `ConcurrencyLimit` ([#518])

[#518]: https://github.com/tower-rs/tower/pull/518
[#516]: https://github.com/tower-rs/tower/pull/516

# 0.4.1 (January 7, 2021)

### Fixed

- Updated `tower-layer` to 0.3.1 to fix broken re-exports.

# 0.4.0 (January 7, 2021)

This is a major breaking release including a large number of changes. In
particular, this release updates `tower` to depend on Tokio 1.0, and moves all
middleware into the `tower` crate. In addition, Tower 0.4 reworks several
middleware APIs, as well as introducing new ones. 

This release does *not* change the core `Service` or `Layer` traits, so `tower`
0.4 still depends on `tower-service` 0.3 and `tower-layer` 0.3. This means that
`tower` 0.4 is still compatible with libraries that depend on those crates.

### Added

- **make**: Added `MakeService::into_service` and `MakeService::as_service` for
  converting `MakeService`s into `Service`s ([#492])
- **steer**: Added `steer` middleware for routing requests to one of a set of
  services ([#426])
- **util**: Added `MapRequest` middleware and `ServiceExt::map_request`, for
  applying a function to a request before passing it to the inner service
  ([#435])
- **util**: Added `MapResponse` middleware and `ServiceExt::map_response`, for
  applying a function to the `Response` type of an inner service after its
  future completes ([#435])
- **util**: Added `MapErr` middleware and `ServiceExt::map_err`, for
  applying a function to the `Error` returned by an inner service if it fails
  ([#396])
- **util**: Added `MapResult` middleware and `ServiceExt::map_result`, for
  applying a function to the `Result` returned by an inner service's future
  regardless of  whether it succeeds or fails ([#499])
- **util**: Added `Then` middleware and `ServiceExt::then`, for chaining another
  future after an inner service's future completes (with a `Response` or an
  `Error`) ([#500])
- **util**: Added `AndThen` middleware and `ServiceExt::and_then`, for
  chaining another future after an inner service's future completes successfully
  ([#485])
- **util**: Added `layer_fn`, for constructing a `Layer` from a function taking
  a `Service` and returning a different `Service` ([#491])
- **util**: Added `FutureService`, which implements `Service` for a
  `Future` whose `Output` type is a `Service` ([#496])
- **util**: Added `BoxService::layer` and `UnsyncBoxService::layer`, to make
  constructing layers more ergonomic ([#503])
- **layer**: Added `Layer` impl for `&Layer` ([#446])
- **retry**: Added `Retry::get_ref`, `Retry::get_mut`, and `Retry::into_inner`
  to access the inner service ([#463])
- **timeout**: Added `Timeout::get_ref`, `Timeout::get_mut`, and
  `Timeout::into_inner` to access the inner service ([#463])
- **buffer**: Added `Clone` and `Copy` impls for `BufferLayer` (#[493])
- Several documentation improvements ([#442], [#444], [#445], [#449], [#487],
  [#490], [#506]])

### Changed

- All middleware `tower-*` crates were merged into `tower` and placed
  behind feature flags ([#432])
- Updated Tokio dependency to 1.0 ([#489])
- **builder**: Make `ServiceBuilder::service` take `self` by reference rather
  than by value ([#504])
- **reconnect**: Return errors from `MakeService` in the response future, rather than
  in `poll_ready`, allowing the reconnect service to be reused when a reconnect
  fails ([#386], [#437])
- **discover**: Changed `Discover` to be a sealed trait alias for a
  `TryStream<Item = Change>`. `Discover` implementations are now written by
  implementing `Stream`. ([#443])
- **load**: Renamed the `Instrument` trait to `TrackCompletion` ([#445])
- **load**: Renamed `NoInstrument` to `CompleteOnResponse` ([#445])
- **balance**: Renamed `BalanceLayer` to `MakeBalanceLayer` ([#449])
- **balance**: Renamed `BalanceMake` to `MakeBalance` ([#449])
- **ready-cache**: Changed `ready_cache::error::Failed`'s `fmt::Debug` impl to
  require the key type to also implement `fmt::Debug` ([#467])
- **filter**: Changed `Filter` and `Predicate` to use a synchronous function as
  a predicate ([#508])
- **filter**: Renamed the previous `Filter` and `Predicate` (where `Predicate`s
  returned a `Future`) to `AsyncFilter` and `AsyncPredicate` ([#508])
- **filter**: `Predicate`s now take a `Request` type by value and may return a
  new request, potentially of a different type ([#508])
- **filter**: `Predicate`s may now return an error of any type ([#508])

### Fixed

- **limit**: Fixed an issue where `RateLimit` services do not reset the remaining
  count when rate limiting ([#438], [#439])
- **util**: Fixed a bug where `oneshot` futures panic if the service does not
  immediately become ready ([#447])
- **ready-cache**: Fixed `ready_cache::error::Failed` not returning inner error types
  via `Error::source` ([#467])
- **hedge**: Fixed an interaction with `buffer` where `buffer` slots were
  eagerly reserved for hedge requests even if they were not sent ([#472])
- **hedge**: Fixed the use of a fixed 10 second bound on the hedge latency
  histogram resulting on errors with longer-lived requests. The latency
  histogram now automatically resizes ([#484])
- **buffer**: Fixed an issue where tasks waiting for buffer capacity were not
  woken when a buffer is dropped, potentially resulting in a task leak ([#480])

### Removed

- Remove `ServiceExt::ready`.
- **discover**: Removed `discover::stream` module, since `Discover` is now an
  alias for `Stream` ([#443])
- **buffer**: Removed `MakeBalance::from_rng`, which caused all balancers to use
  the same RNG ([#497])

[#432]: https://github.com/tower-rs/tower/pull/432
[#426]: https://github.com/tower-rs/tower/pull/426
[#435]: https://github.com/tower-rs/tower/pull/435
[#499]: https://github.com/tower-rs/tower/pull/499
[#386]: https://github.com/tower-rs/tower/pull/386
[#437]: https://github.com/tower-rs/tower/pull/487
[#438]: https://github.com/tower-rs/tower/pull/438
[#437]: https://github.com/tower-rs/tower/pull/439
[#443]: https://github.com/tower-rs/tower/pull/443
[#442]: https://github.com/tower-rs/tower/pull/442
[#444]: https://github.com/tower-rs/tower/pull/444
[#445]: https://github.com/tower-rs/tower/pull/445
[#446]: https://github.com/tower-rs/tower/pull/446
[#447]: https://github.com/tower-rs/tower/pull/447
[#449]: https://github.com/tower-rs/tower/pull/449
[#463]: https://github.com/tower-rs/tower/pull/463
[#396]: https://github.com/tower-rs/tower/pull/396
[#467]: https://github.com/tower-rs/tower/pull/467
[#472]: https://github.com/tower-rs/tower/pull/472
[#480]: https://github.com/tower-rs/tower/pull/480
[#484]: https://github.com/tower-rs/tower/pull/484
[#489]: https://github.com/tower-rs/tower/pull/489
[#497]: https://github.com/tower-rs/tower/pull/497
[#487]: https://github.com/tower-rs/tower/pull/487
[#493]: https://github.com/tower-rs/tower/pull/493
[#491]: https://github.com/tower-rs/tower/pull/491
[#495]: https://github.com/tower-rs/tower/pull/495
[#503]: https://github.com/tower-rs/tower/pull/503
[#504]: https://github.com/tower-rs/tower/pull/504
[#492]: https://github.com/tower-rs/tower/pull/492
[#500]: https://github.com/tower-rs/tower/pull/500
[#490]: https://github.com/tower-rs/tower/pull/490
[#506]: https://github.com/tower-rs/tower/pull/506
[#508]: https://github.com/tower-rs/tower/pull/508
[#485]: https://github.com/tower-rs/tower/pull/485

# 0.3.1 (January 17, 2020)

- Allow opting out of tracing/log (#410).

# 0.3.0 (December 19, 2019)

- Update all tower based crates to `0.3`.
- Update to `tokio 0.2`
- Update to `futures 0.3`

# 0.3.0-alpha.2 (September 30, 2019)

- Move to `futures-*-preview 0.3.0-alpha.19`
- Move to `pin-project 0.4`

# 0.3.0-alpha.1a (September 13, 2019)

- Update `tower-buffer` to `0.3.0-alpha.1b`

# 0.3.0-alpha.1 (September 11, 2019)

- Move to `std::future`

# 0.1.1 (July 19, 2019)

- Add `ServiceBuilder::into_inner`

# 0.1.0 (April 26, 2019)

- Initial release
