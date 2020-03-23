use super::Policy;
use futures_util::future;

/// A very basic retry policy with a limited number of retry attempts.
///
/// FIXME: explain why not to use this.
#[derive(Clone, Debug)]
pub struct RetryLimit {
    remaining_tries: usize,
}

impl RetryLimit {
    /// Create a policy with the given number of retry attempts.
    pub fn new(retry_attempts: usize) -> Self {
        RetryLimit {
            remaining_tries: retry_attempts,
        }
    }
}

impl<Req: Clone, Res, E> Policy<Req, Res, E> for RetryLimit {
    type Future = future::Ready<Self>;
    fn retry(&self, _: &Req, result: Result<&Res, &E>) -> Option<Self::Future> {
        if result.is_err() {
            if self.remaining_tries > 0 {
                Some(future::ready(RetryLimit {
                    remaining_tries: self.remaining_tries - 1,
                }))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn clone_request(&self, req: &Req) -> Option<Req> {
        Some(req.clone())
    }
}

/// A very basic retry policy that always retries failed requests.
///
/// FIXME: explain why not to use this.
#[derive(Clone, Debug)]
pub struct RetryErrors;

impl<Req: Clone, Res, E> Policy<Req, Res, E> for RetryErrors {
    type Future = future::Ready<Self>;
    fn retry(&self, _: &Req, result: Result<&Res, &E>) -> Option<Self::Future> {
        if result.is_err() {
            Some(future::ready(RetryErrors))
        } else {
            None
        }
    }

    fn clone_request(&self, req: &Req) -> Option<Req> {
        Some(req.clone())
    }
}
