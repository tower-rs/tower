# 0.1.2 (April 15, 2020)

- Fixed an issue where the remaining available requests within a period could get passed
    to the next period. This could only happen if the timer is reset in `Serivce::call`
    and there were more than 1 available remaining requests.

# 0.1.1 (October 11, 2019)

- Added `tracing` events for when requests are limited

# 0.1.0 (April 26, 2019)

- Initial release
