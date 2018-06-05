use futures::{Async, Poll};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::clock;
use tower_discover::{Change, Discover};
use tower_service::Service;

use super::{Measure, MeasureFuture, NoMeasure};

use Load;

/// Wraps an `S`-typed Service with Peak-EWMA load measurement.
///
/// `PeakEwma` implements `Load` with the `Cost` metric that estimates the amount of
/// pending work to an endpoint. Work is calculated by multiplying the
/// exponentially-weighted moving average (EWMA) of response latencies by the number of
/// pending requests. The Peak-EWMA algorithm is designed ot be especially sensitive to
/// worst-case latencies. Over time, the peak latency value decays towards the moving
/// average of latencies to the endpoint.
///
/// As requests are sent to the underlying service, an `M`-typed measurement strategy is
/// used to instrument responses to measure latency in an application-specific way. The
/// default strategy measures latency as the elapsed time from the request being issued to
/// the underlying service to the response future being satisfied (or dropped).
///
/// When no latency information has been measured for an endpoint, the [standard RTT
/// within a datacenter][numbers] (500ns) is used.
///
/// [numbers]: https://people.eecs.berkeley.edu/~rcs/research/interactive_latency.html
pub struct PeakEwma<S, M = NoMeasure> {
    service: S,
    decay_ns: f64,
    rtt_estimate: Arc<Mutex<Option<RttEstimate>>>,
    _p: PhantomData<M>,
}

/// Wraps a `D`-typed stream of discovery updates with `PeakEwma`.
pub struct WithPeakEwma<D, M = NoMeasure> {
    discover: D,
    decay_ns: f64,
    _p: PhantomData<M>,
}

/// Represents the relative cost of communicating with a service.
///
/// The underlying value estimates the amount of pending work to a service: the Peak-EWMA
/// latency estimate multiplied by the number of pending requests.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct Cost(f64);

/// Updates `RttEstimate` when dropped.
pub struct Instrument {
    sent_at: Instant,
    decay_ns: f64,
    rtt_estimate: Arc<Mutex<Option<RttEstimate>>>,
}

/// Holds the current RTT estimateand the last time this value was updated.
struct RttEstimate {
    update_at: Instant,
    rtt_ns: f64,
}

/// The "standard" RTT within a datacenter is ~500ns.
///
/// https://people.eecs.berkeley.edu/~rcs/research/interactive_latency.html
const DEFAULT_RTT_ESTIMATE: f64 = 500_000.0;

const NANOS_PER_MILLI: f64 = 1_000_000.0;

// ===== impl PeakEwma =====

impl<D: Discover> WithPeakEwma<D, NoMeasure> {
    pub fn new(discover: D, decay: Duration) -> Self {
        Self {
            discover,
            decay_ns: nanos(decay),
            _p: PhantomData,
        }
    }

    /// Configures `WithPeakEwma` to use an `M`-typed measurement strategy.
    pub fn measured<M>(self) -> WithPeakEwma<D, M>
    where
        M: Measure<Instrument, D::Response>,
    {
        WithPeakEwma {
            discover: self.discover,
            decay_ns: self.decay_ns,
            _p: PhantomData,
        }
    }
}

impl<D, M> Discover for WithPeakEwma<D, M>
where
    D: Discover,
    M: Measure<Instrument, D::Response>,
{
    type Key = D::Key;
    type Request = D::Request;
    type Response = M::Measured;
    type Error = D::Error;
    type Service = PeakEwma<D::Service, M>;
    type DiscoverError = D::DiscoverError;

    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::DiscoverError> {
        use self::Change::*;

        let change = match try_ready!(self.discover.poll()) {
            Insert(k, svc) => Insert(k, PeakEwma::new(svc, self.decay_ns)),
            Remove(k) => Remove(k),
        };

        Ok(Async::Ready(change))
    }
}

// ===== impl PeakEwma =====

impl<S, M> PeakEwma<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    fn new(service: S, decay_ns: f64) -> Self {
        Self {
            service,
            decay_ns,
            rtt_estimate: Arc::new(Mutex::new(None)),
            _p: PhantomData,
        }
    }

    fn instrument(&self) -> Instrument {
        Instrument {
            decay_ns: self.decay_ns,
            sent_at: clock::now(),
            rtt_estimate: self.rtt_estimate.clone(),
        }
    }
}

impl<S, M> Service for PeakEwma<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    type Request = S::Request;
    type Response = M::Measured;
    type Error = S::Error;
    type Future = MeasureFuture<S::Future, M, Instrument>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        MeasureFuture::new(self.instrument(), self.service.call(req))
    }
}

impl<S, M> Load for PeakEwma<S, M> {
    type Metric = Cost;

    fn load(&self) -> Self::Metric {
        let pending = Arc::strong_count(&self.rtt_estimate) as u32 - 1;

        // Update the RTT estimate to account for decay since the last update.
        // If an estimate has not been established, a default is provided
        let estimate = {
            let mut rtt = self.rtt_estimate.lock().expect("peak ewma prior_estimate");
            match *rtt {
                Some(ref mut rtt) => rtt.decay(self.decay_ns),
                None => DEFAULT_RTT_ESTIMATE,
            }
        };

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

// ===== impl RttEstimate =====

impl RttEstimate {
    fn new(sent_at: Instant, recv_at: Instant) -> Self {
        debug_assert!(
            sent_at <= recv_at,
            "recv_at={:?} after sent_at={:?}",
            recv_at,
            sent_at
        );

        Self {
            rtt_ns: nanos(recv_at - sent_at),
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
            self.update_at < now,
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

// ===== impl Instrument =====

impl Drop for Instrument {
    fn drop(&mut self) {
        let recv_at = clock::now();

        if let Ok(mut rtt) = self.rtt_estimate.lock() {
            if let Some(ref mut rtt) = *rtt {
                rtt.update(self.sent_at, recv_at, self.decay_ns);
                return;
            }

            *rtt = Some(RttEstimate::new(self.sent_at, recv_at));
        }
    }
}

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
    use super::*;

    #[test]
    fn test_nanos() {
        assert_eq!(nanos(Duration::new(0, 0)), 0.0);
        assert_eq!(nanos(Duration::new(0, 123)), 123.0);
        assert_eq!(nanos(Duration::new(1, 23)), 1_000_000_023.0);
        assert_eq!(
            nanos(Duration::new(::std::u64::MAX, 999_999_999)),
            18446744074709553000.0
        );
    }
}
