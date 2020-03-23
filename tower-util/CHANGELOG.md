# 0.3.1 (March 23, 2020)

- Adds `ReadyAnd` and `ServiceExt::ready_and`, which yield `&mut` to the
  service when it is ready (#427).
- Adds `ReadyOneshot` and `ServiceExt::ready_oneshot`, which yield the
  service when it is ready (#427).
- Updates `Ready` and `ServiceExt::ready` documentation to reflect that
  they do not yield the service, just unit, when the service is ready
  (#427).

# 0.3.0 (December 19, 2019)

- Update to `tower-serivce 0.3`
- Update to `futures 0.3`
- Update to `tokio 0.2`

# 0.3.0-alpha.2 (September 30, 2019)

- Move to `futures-*-preview 0.3.0-alpha.19`
- Move to `pin-project 0.4`

# 0.3.0-alpha.1 (September 11, 2019)

- Move to `std::future`

# 0.1.0 (April 26, 2019)

- Initial release
