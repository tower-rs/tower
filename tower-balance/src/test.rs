use futures::{future, Async, Poll};
use quickcheck::*;
use std::collections::VecDeque;
use tower_discover::Change;
use tower_service::Service;

use crate::*;

type Error = Box<dyn std::error::Error + Send + Sync>;

struct ReluctantDisco(VecDeque<Change<usize, ReluctantService>>);

struct ReluctantService {
    polls_until_ready: usize,
}

impl Discover for ReluctantDisco {
    type Key = usize;
    type Service = ReluctantService;
    type Error = Error;

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::Error> {
        let r = self
            .0
            .pop_front()
            .map(Async::Ready)
            .unwrap_or(Async::NotReady);
        debug!("polling disco: {:?}", r.is_ready());
        Ok(r)
    }
}

impl Service<()> for ReluctantService {
    type Response = ();
    type Error = Error;
    type Future = future::FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if self.polls_until_ready == 0 {
            return Ok(Async::Ready(()));
        }

        self.polls_until_ready -= 1;
        return Ok(Async::NotReady);
    }

    fn call(&mut self, _: ()) -> Self::Future {
        future::ok(())
    }
}

quickcheck! {
    /// Creates a random number of services, each of which must be polled a random
    /// number of times before becoming ready. As the balancer is polled, ensure that
    /// it does not become ready prematurely and that services are promoted from
    /// not_ready to ready.
    fn poll_ready(service_tries: Vec<usize>) -> TestResult {
        // Stores the number of pending services after each poll_ready call.
        let mut pending_at = Vec::new();

        let disco = {
            let mut changes = VecDeque::new();
            for (i, n) in service_tries.iter().map(|n| *n).enumerate() {
                for j in 0..n {
                    if j == pending_at.len() {
                        pending_at.push(1);
                    } else {
                        pending_at[j] += 1;
                    }
                }

                let s = ReluctantService { polls_until_ready: n };
                changes.push_back(Change::Insert(i, s));
            }
            ReluctantDisco(changes)
        };
        pending_at.push(0);

        let mut balancer = Balance::new(disco, choose::RoundRobin::default());

        let services = service_tries.len();
        let mut next_pos = 0;
        for pending in pending_at.iter().map(|p| *p) {
            assert!(pending <= services);
            let ready = services - pending;

            match balancer.poll_ready() {
                Err(_) => return TestResult::error("poll_ready failed"),
                Ok(p) => {
                    if p.is_ready() != (ready > 0) {
                        return TestResult::failed();
                    }
                }
            }

            if balancer.num_ready() != ready {
                return TestResult::failed();
            }

            if balancer.num_not_ready() != pending {
                return TestResult::failed();
            }

            if balancer.is_ready() != (ready > 0) {
                return TestResult::failed();
            }
            if balancer.is_not_ready() != (ready == 0) {
                return TestResult::failed();
            }

            if balancer.dispatched_ready_index.is_some() {
                return TestResult::failed();
            }

            if ready == 0 {
                if balancer.chosen_ready_index.is_some() {
                    return TestResult::failed();
                }
            } else {
                // Check that the round-robin chooser is doing its thing:
                match balancer.chosen_ready_index {
                    None => return TestResult::failed(),
                    Some(idx) => {
                        if idx != next_pos  {
                            return TestResult::failed();
                        }
                    }
                }

                next_pos = (next_pos + 1) % ready;
            }
        }

        TestResult::passed()
    }
}
