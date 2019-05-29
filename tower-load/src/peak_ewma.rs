//! A `Load` implementation that PeakEWMA on response latency.

use super::{Instrument, InstrumentFuture, NoInstrument};
use crate::Load;
use futures::{try_ready, Async, Poll};
use log::trace;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_timer::clock;
use tower_discover::{Change, Discover};
use tower_service::Service;

/// Wraps an `S`-typed Service with Peak-EWMA load measurement.
///
/// `PeakEwma` implements `Load` with the `Cost` metric that estimates the amount of
/// pending work to an endpoint. Work is calculated by multiplying the
/// exponentially-weighted moving average (EWMA) of response latencies by the number of
/// pending requests. The Peak-EWMA algorithm is designed to be especially sensitive to
/// worst-case latencies. Over time, the peak latency value decays towards the moving
/// average of latencies to the endpoint.
///
/// As requests are sent to the underlying service, an `I`-typed instrumentation strategy
/// is used to track responses to measure latency in an application-specific way. The
/// default strategy measures latency as the elapsed time from the request being issued to
/// the underlying service to the response future being satisfied (or dropped).
///
/// When no latency information has been measured for an endpoint, an arbitrary default
/// RTT of 1 second is used to prevent the endpoint from being overloaded before a
/// meaningful baseline can be established..
///
/// ## Note
///
/// This is derived from [Finagle][finagle], which is distributed under the Apache V2
/// license. Copyright 2017, Twitter Inc.
///
/// [finagle]:
/// https://github.com/twitter/finagle/blob/9cc08d15216497bb03a1cafda96b7266cfbbcff1/finagle-core/src/main/scala/com/twitter/finagle/loadbalancer/PeakEwma.scala
pub struct PeakEwma<S, I = NoInstrument> {
    service: S,
    decay_ns: f64,
    rtt_estimate: Arc<Mutex<RttEstimate>>,
    instrument: I,
}

/// Wraps a `D`-typed stream of discovery updates with `PeakEwma`.
pub struct PeakEwmaDiscover<D, I = NoInstrument> {
    discover: D,
    decay_ns: f64,
    default_rtt: Duration,
    instrument: I,
}

/// Represents the relative cost of communicating with a service.
///
/// The underlying value estimates the amount of pending work to a service: the Peak-EWMA
/// latency estimate multiplied by the number of pending requests.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct Cost(f64);

/// Tracks an in-flight request and updates the RTT-estimate on Drop.
pub struct Handle {
    sent_at: Instant,
    decay_ns: f64,
    rtt_estimate: Arc<Mutex<RttEstimate>>,
}

/// Holds the current RTT estimate and the last time this value was updated.
struct RttEstimate {
    update_at: Instant,
    rtt_ns: f64,
}

const NANOS_PER_MILLI: f64 = 1_000_000.0;

// ===== impl PeakEwma =====

impl<D, I> PeakEwmaDiscover<D, I> {
    /// Wraps a `D`-typed `Discover` so that services have a `PeakEwma` load metric.
    ///
    /// The provided `default_rtt` is used as the default RTT estimate for newly
    /// added services.
    ///
    /// They `decay` value determines over what time period a RTT estimate should
    /// decay.
    pub fn new<Request>(discover: D, default_rtt: Duration, decay: Duration, instrument: I) -> Self
    where
        D: Discover,
        D::Service: Service<Request>,
        I: Instrument<Handle, <D::Service as Service<Request>>::Response>,
    {
        PeakEwmaDiscover {
            discover,
            decay_ns: nanos(decay),
            default_rtt,
            instrument,
        }
    }
}

impl<D, I> Discover for PeakEwmaDiscover<D, I>
where
    D: Discover,
    I: Clone,
{
    type Key = D::Key;
    type Service = PeakEwma<D::Service, I>;
    type Error = D::Error;

    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::Error> {
        let change = match try_ready!(self.discover.poll()) {
            Change::Remove(k) => Change::Remove(k),
            Change::Insert(k, svc) => {
                let peak_ewma = PeakEwma::new(
                    svc,
                    self.default_rtt,
                    self.decay_ns,
                    self.instrument.clone(),
                );
                Change::Insert(k, peak_ewma)
            }
        };

        Ok(Async::Ready(change))
    }
}

// ===== impl PeakEwma =====

impl<S, I> PeakEwma<S, I> {
    fn new(service: S, default_rtt: Duration, decay_ns: f64, instrument: I) -> Self {
        Self {
            service,
            decay_ns,
            rtt_estimate: Arc::new(Mutex::new(RttEstimate::new(nanos(default_rtt)))),
            instrument,
        }
    }

    fn handle(&self) -> Handle {
        Handle {
            decay_ns: self.decay_ns,
            sent_at: clock::now(),
            rtt_estimate: self.rtt_estimate.clone(),
        }
    }
}

impl<S, I, Request> Service<Request> for PeakEwma<S, I>
where
    S: Service<Request>,
    I: Instrument<Handle, S::Response>,
{
    type Response = I::Output;
    type Error = S::Error;
    type Future = InstrumentFuture<S::Future, I, Handle>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        InstrumentFuture::new(
            self.instrument.clone(),
            self.handle(),
            self.service.call(req),
        )
    }
}

impl<S, I> Load for PeakEwma<S, I> {
    type Metric = Cost;

    fn load(&self) -> Self::Metric {
        let pending = Arc::strong_count(&self.rtt_estimate) as u32 - 1;

        // Update the RTT estimate to account for decay since the last update.
        // If an estimate has not been established, a default is provided
        let estimate = self.update_estimate();

        let cost = Cost(estimate * f64::from(pending + 1));
        trace!(
            "load estimate={:.0}ms pending={} cost={:?}",
            estimate / NANOS_PER_MILLI,
            pending,
            cost,
        );
        cost
    }
}

impl<S, I> PeakEwma<S, I> {
    fn update_estimate(&self) -> f64 {
        let mut rtt = self.rtt_estimate.lock().expect("peak ewma prior_estimate");
        rtt.decay(self.decay_ns)
    }
}

// ===== impl RttEstimate =====

impl RttEstimate {
    fn new(rtt_ns: f64) -> Self {
        debug_assert!(0.0 < rtt_ns, "rtt must be positive");
        Self {
            rtt_ns,
            update_at: clock::now(),
        }
    }

    /// Decays the RTT estimate with a decay period of `decay_ns`.
    fn decay(&mut self, decay_ns: f64) -> f64 {
        // Updates with a 0 duration so that the estimate decays towards 0.
        let now = clock::now();
        self.update(now, now, decay_ns)
    }

    /// Updates the Peak-EWMA RTT estimate.
    ///
    /// The elapsed time from `sent_at` to `recv_at` is added
    fn update(&mut self, sent_at: Instant, recv_at: Instant, decay_ns: f64) -> f64 {
        debug_assert!(
            sent_at <= recv_at,
            "recv_at={:?} after sent_at={:?}",
            recv_at,
            sent_at
        );
        let rtt = nanos(recv_at - sent_at);

        let now = clock::now();
        debug_assert!(
            self.update_at <= now,
            "update_at={:?} in the future",
            self.update_at
        );

        self.rtt_ns = if self.rtt_ns < rtt {
            // For Peak-EWMA, always use the worst-case (peak) value as the estimate for
            // subsequent requests.
            trace!(
                "update peak rtt={}ms prior={}ms",
                rtt / NANOS_PER_MILLI,
                self.rtt_ns / NANOS_PER_MILLI,
            );
            rtt
        } else {
            // When an RTT is observed that is less than the estimated RTT, we decay the
            // prior estimate according to how much time has elapsed since the last
            // update. The inverse of the decay is used to scale the estimate towards the
            // observed RTT value.
            let elapsed = nanos(now - self.update_at);
            let decay = (-elapsed / decay_ns).exp();
            let recency = 1.0 - decay;
            let next_estimate = (self.rtt_ns * decay) + (rtt * recency);
            trace!(
                "update rtt={:03.0}ms decay={:06.0}ns; next={:03.0}ms",
                rtt / NANOS_PER_MILLI,
                self.rtt_ns - next_estimate,
                next_estimate / NANOS_PER_MILLI,
            );
            next_estimate
        };
        self.update_at = now;

        self.rtt_ns
    }
}

// ===== impl Handle =====

impl Drop for Handle {
    fn drop(&mut self) {
        let recv_at = clock::now();

        if let Ok(mut rtt) = self.rtt_estimate.lock() {
            rtt.update(self.sent_at, recv_at, self.decay_ns);
        }
    }
}

// ===== impl Cost =====

// Utility that converts durations to nanos in f64.
//
// Due to a lossy transformation, the maximum value that can be represented is ~585 years,
// which, I hope, is more than enough to represent request latencies.
fn nanos(d: Duration) -> f64 {
    const NANOS_PER_SEC: u64 = 1_000_000_000;
    let n = f64::from(d.subsec_nanos());
    let s = d.as_secs().saturating_mul(NANOS_PER_SEC) as f64;
    n + s
}

#[cfg(test)]
mod tests {
    use futures::{future, Future, Poll};
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    use tokio_executor::enter;
    use tokio_timer::clock;

    use super::*;

    struct Svc;
    impl Service<()> for Svc {
        type Response = ();
        type Error = ();
        type Future = future::FutureResult<(), ()>;

        fn poll_ready(&mut self) -> Poll<(), ()> {
            Ok(().into())
        }

        fn call(&mut self, (): ()) -> Self::Future {
            future::ok(())
        }
    }

    struct Now(Arc<Mutex<Instant>>);
    impl clock::Now for Now {
        fn now(&self) -> Instant {
            *self.0.lock().expect("now")
        }
    }

    /// The default RTT estimate decays, so that new nodes are considered if the
    /// default RTT is too high.
    #[test]
    fn default_decay() {
        let time = Arc::new(Mutex::new(Instant::now()));
        let clock = clock::Clock::new_with_now(Now(time.clone()));

        let mut enter = enter().expect("enter");
        clock::with_default(&clock, &mut enter, |_| {
            let svc = PeakEwma::new(
                Svc,
                Duration::from_millis(10),
                NANOS_PER_MILLI * 1_000.0,
                NoInstrument,
            );
            let Cost(load) = svc.load();
            assert_eq!(load, 10.0 * NANOS_PER_MILLI);

            *time.lock().unwrap() += Duration::from_millis(100);
            let Cost(load) = svc.load();
            assert!(9.0 * NANOS_PER_MILLI < load && load < 10.0 * NANOS_PER_MILLI);

            *time.lock().unwrap() += Duration::from_millis(100);
            let Cost(load) = svc.load();
            assert!(8.0 * NANOS_PER_MILLI < load && load < 9.0 * NANOS_PER_MILLI);
        });
    }

    /// The default RTT estimate decays, so that new nodes are considered if the
    /// default RTT is too high.
    #[test]
    fn compound_decay() {
        let time = Arc::new(Mutex::new(Instant::now()));
        let clock = clock::Clock::new_with_now(Now(time.clone()));

        let mut enter = enter().expect("enter");
        clock::with_default(&clock, &mut enter, |_| {
            let mut svc = PeakEwma::new(
                Svc,
                Duration::from_millis(20),
                NANOS_PER_MILLI * 1_000.0,
                NoInstrument,
            );
            assert_eq!(svc.load(), Cost(20.0 * NANOS_PER_MILLI));

            *time.lock().unwrap() += Duration::from_millis(100);
            let rsp0 = svc.call(());
            assert!(svc.load() > Cost(20.0 * NANOS_PER_MILLI));

            *time.lock().unwrap() += Duration::from_millis(100);
            let rsp1 = svc.call(());
            assert!(svc.load() > Cost(40.0 * NANOS_PER_MILLI));

            *time.lock().unwrap() += Duration::from_millis(100);
            let () = rsp0.wait().unwrap();
            assert_eq!(svc.load(), Cost(400_000_000.0));

            *time.lock().unwrap() += Duration::from_millis(100);
            let () = rsp1.wait().unwrap();
            assert_eq!(svc.load(), Cost(200_000_000.0));

            // Check that values decay as time elapses
            *time.lock().unwrap() += Duration::from_secs(1);
            assert!(svc.load() < Cost(100_000_000.0));

            *time.lock().unwrap() += Duration::from_secs(10);
            assert!(svc.load() < Cost(100_000.0));
        });
    }

    #[test]
    fn nanos() {
        assert_eq!(super::nanos(Duration::new(0, 0)), 0.0);
        assert_eq!(super::nanos(Duration::new(0, 123)), 123.0);
        assert_eq!(super::nanos(Duration::new(1, 23)), 1_000_000_023.0);
        assert_eq!(
            super::nanos(Duration::new(::std::u64::MAX, 999_999_999)),
            18446744074709553000.0
        );
    }
}
