/// Maintain an exponential moving average of Long-typed values over a
/// given window on a user-defined clock.
///
/// Ema requires monotonic timestamps
#[derive(Debug)]
pub struct Ema {
    window: u64,
    timestamp: u64,
    ema: f64,
}

impl Ema {
    /// Constructs a new `Ema` instance.
    ///
    /// * `window` - The mean lifetime of observations.
    pub fn new(window: u64) -> Self {
        Ema {
            window,
            timestamp: 0,
            ema: 0.0,
        }
    }

    /// `true` if `Ema` contains no values.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.timestamp == 0
    }

    /// Updates the average with observed value. and return the new average.
    ///
    /// Since `update` requires monotonic timestamps. it is up to the caller to
    /// ensure that calls to update do not race.
    ///
    /// # Panics
    ///
    /// When timestamp isn't monotonic.
    pub fn update(&mut self, timestamp: u64, value: f64) -> f64 {
        if self.timestamp == 0 {
            self.timestamp = timestamp;
            self.ema = value;
        } else {
            assert!(
                timestamp >= self.timestamp,
                "non monotonic timestamp detected"
            );
            let time_diff = timestamp - self.timestamp;
            self.timestamp = timestamp;

            let w = if self.window == 0 {
                0_f64
            } else {
                (-(time_diff as f64) / self.window as f64).exp()
            };

            self.ema = value * (1_f64 - w) + self.ema * w;
        }

        self.ema
    }

    /// Returns the last observation.
    #[allow(dead_code)]
    pub fn last(&self) -> f64 {
        self.ema
    }

    /// Resets the average to 0 and erase all observations.
    pub fn reset(&mut self) {
        self.timestamp = 0;
        self.ema = 0_f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute() {
        // http://stockcharts.com/school/doku.php?id=chart_school:technical_indicators:moving_averages
        const PER_DAY_DATA: [f64; 30] = [
            22.27, 22.19, 22.08, 22.17, 22.18, 22.13, 22.23, 22.43, 22.24, 22.29, 22.15, 22.39,
            22.38, 22.61, 23.36, 24.05, 23.75, 23.83, 23.95, 23.63, 23.82, 23.87, 23.65, 23.19,
            23.10, 23.33, 22.68, 23.10, 22.40, 22.17,
        ];

        const EMA_10_OVER_DAYS: [f64; 30] = [
            22.27, 22.26, 22.25, 22.24, 22.23, 22.22, 22.22, 22.24, 22.24, 22.25, 22.24, 22.25,
            22.26, 22.3, 22.4, 22.56, 22.67, 22.78, 22.89, 22.96, 23.04, 23.12, 23.17, 23.17,
            23.17, 23.18, 23.13, 23.13, 23.06, 22.98,
        ];

        let mut ema = Ema::new(10);

        let result = PER_DAY_DATA
            .iter()
            .enumerate()
            .map(|(i, x)| ((i + 1) as u64, x))
            .map(|(i, x)| round_to(ema.update(i, *x), 2))
            .collect::<Vec<_>>();

        assert_eq!(EMA_10_OVER_DAYS.to_vec(), result);
    }

    #[test]
    #[should_panic]
    fn non_monotonic_timestamp() {
        let mut ema = Ema::new(10);
        ema.update(10, 10.0);
        ema.update(1, 10.0);
    }

    #[test]
    fn updates_are_time_invariant() {
        let mut a = Ema::new(1000);
        let mut b = Ema::new(1000);

        assert_eq!(10.0, a.update(10, 10.0));
        assert_eq!(10.0, a.update(20, 10.0));
        assert_eq!(10.0, a.update(30, 10.0));

        assert_eq!(10.0, b.update(10, 10.0));
        assert_eq!(10.0, b.update(30, 10.0));

        assert_eq!(a.update(40, 5.0), b.update(40, 5.0));
        assert!(a.update(50, 5.0) > b.update(60, 5.0));

        assert_eq!(
            round_to(a.update(60, 5.0), 4),
            round_to(b.update(60, 5.0), 4)
        );
    }

    #[test]
    fn reset() {
        let mut ema = Ema::new(5);

        assert_eq!(3.0, ema.update(1, 3.0));

        ema.reset();

        assert!(ema.is_empty());
        assert_eq!(0.0, ema.last());
        assert_eq!(5.0, ema.update(2, 5.0));
    }

    fn round_to(x: f64, power: i32) -> f64 {
        let power = f64::powi(10.0, power);
        (x * power).round() / power
    }
}
