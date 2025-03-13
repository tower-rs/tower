//! A standard retry policy that combines many of the `retry` module's utilities
//! together in a production-ready retry policy.
//!
//! # Defaults
//!
//! - [`ExponentialBackoffMaker`] is the default type for `B`.
//! - [`Budget`]'s default implementation is used.
//! - [`IsRetryable`] by default will always return `false`.
//! - [`CloneRequest`] by default will always return `None`.
//!
//! # Backoff
//!
//! The [`StandardRetryPolicy`] takes some [`MakeBackoff`] and will make a
//! [`Backoff`] for each request "session" (a request session is the initial
//! request and any subsequent requests). It will then supply the backoff future
//! to the retry middlware. Usually, this future is the [`tokio::time::Sleep`]
//! type.
//!
//! # Budget
//!
//! The [`StandardRetryPolicy`] uses the [`Budget`] type to ensure that for each
//! produced policy that this client will not overwhelm downstream services.
//! Check the docs of [`Budget`] to understand what the defaults are and why they
//! were chosen.
//!
//! # Example
//!
//! ```
//!# use tower::retry::standard_policy::StandardRetryPolicy;
//!# use tower::retry::budget::Budget;
//!
//! let policy = StandardRetryPolicy::<(), (), ()>::builder()
//!     .should_retry(|res: &mut Result<(), ()>| true)
//!     .clone_request(|req: &()| Some(*req))
//!     .budget(Budget::default())
//!     .build();
//! ```
use std::{fmt, marker::PhantomData, sync::Arc};

use super::{
    backoff::{Backoff, ExponentialBackoffMaker, MakeBackoff},
    budget::Budget,
};
use crate::retry::Policy;

/// A trait to determine if the request associated with the response should be
/// retried by [`StandardRetryPolicy`].
///
/// # Closure
///
/// This trait provides a blanket implementation for a closure of the type
/// `Fn(&mut Result<Res, E>) -> bool + Send + Sync + 'static`.
pub trait IsRetryable<Res, E>: Send + Sync + 'static {
    /// Return `true` if the request associated with the response should be
    /// retried and `false` if it should not be retried.
    fn is_retryable(&self, response: &mut Result<Res, E>) -> bool;
}

/// A trait to clone a request for the [`StandardRetryPolicy`].
///
/// # Closure
///
/// This trait provides a blanket implementation for a closure of the type
/// `Fn(&Req) -> Option<Req> + Send + Sync + 'static`.
pub trait CloneRequest<Req>: Send + Sync + 'static {
    /// Clone a request, if `None` is returned the request will not be retried.
    fn clone_request(&self, request: &Req) -> Option<Req>;
}

impl<Res, E, F> IsRetryable<Res, E> for F
where
    F: Fn(&mut Result<Res, E>) -> bool + Send + Sync + 'static,
{
    fn is_retryable(&self, response: &mut Result<Res, E>) -> bool {
        (self)(response)
    }
}

impl<Req, F> CloneRequest<Req> for F
where
    F: Fn(&Req) -> Option<Req> + Send + Sync + 'static,
{
    fn clone_request(&self, request: &Req) -> Option<Req> {
        (self)(request)
    }
}

/// A standard retry [`Policy`] that combines a retry budget and a backoff
/// mechanism to produce a safe retry policy that prevents congestive collapse
/// and retry storms.
///
/// This type is constructed with the [`StandardRetryPolicyBuilder`].
pub struct StandardRetryPolicy<Req, Res, E, B = ExponentialBackoffMaker>
where
    B: MakeBackoff,
{
    is_retryable: Arc<dyn IsRetryable<Res, E>>,
    clone_request: Arc<dyn CloneRequest<Req>>,
    budget: Arc<Budget>,
    make_backoff: B,
    current_backoff: B::Backoff,
    _pd: PhantomData<fn(Req, Res, E)>,
}

/// Builder for [`StandardRetryPolicy`].
///
/// This type can constructed from the `StandardRetryPolicy::builder` function.
pub struct StandardRetryPolicyBuilder<Req, Res, E, B = ExponentialBackoffMaker> {
    is_retryable: Arc<dyn IsRetryable<Res, E>>,
    clone_request: Arc<dyn CloneRequest<Req>>,
    make_backoff: B,
    budget: Arc<Budget>,
}

impl<Req, Res, E, B: MakeBackoff> StandardRetryPolicyBuilder<Req, Res, E, B> {
    /// Sets the retry decision maker.
    ///
    /// # Default
    ///
    /// By default, this will be set to an [`IsRetryable`] implementation that
    /// always returns false and thus will not retry requests.
    ///
    /// # Example
    ///
    /// ```
    /// # use tower::retry::standard_policy::StandardRetryPolicy;
    /// // Set the Req, Res, and E type to () for simplicity, replace these with
    /// // your specific request/response/error types.
    /// StandardRetryPolicy::<(), (), ()>::builder()
    ///     .should_retry(|res: &mut Result<(), ()>| true)
    ///     .build();
    /// ```
    pub fn should_retry(mut self, f: impl IsRetryable<Res, E> + 'static) -> Self {
        self.is_retryable = Arc::new(f);
        self
    }

    /// Sets the clone request handler.
    ///
    /// # Default
    ///
    /// By default, this will be set to a [`CloneRequest`] implementation that
    /// will never clone the request and will always return `None`.
    ///
    /// # Example
    ///
    /// ```
    /// # use tower::retry::standard_policy::StandardRetryPolicy;
    /// // Set the Req, Res, and E type to () for simplicity, replace these with
    /// // your specific request/response/error types.
    /// StandardRetryPolicy::<(), (), ()>::builder()
    ///     .clone_request(|req: &()| Some(*req))
    ///     .build();
    /// ```
    pub fn clone_request(mut self, f: impl CloneRequest<Req> + 'static) -> Self {
        self.clone_request = Arc::new(f);
        self
    }

    /// Sets the backoff maker.
    ///
    /// # Default
    ///
    /// By default, this will be set to [`ExponentialBackoffMaker`]'s default
    /// implementation.
    ///
    /// # Example
    ///
    /// ```
    /// # use tower::retry::standard_policy::StandardRetryPolicy;
    /// # use tower::retry::backoff::ExponentialBackoffMaker;
    /// // Set the Req, Res, and E type to () for simplicity, replace these with
    /// // your specific request/response/error types.
    /// StandardRetryPolicy::<(), (), ()>::builder()
    ///     .make_backoff(ExponentialBackoffMaker::default())
    ///     .build();
    /// ```
    pub fn make_backoff<B2>(self, backoff: B2) -> StandardRetryPolicyBuilder<Req, Res, E, B2> {
        StandardRetryPolicyBuilder {
            make_backoff: backoff,
            is_retryable: self.is_retryable,
            clone_request: self.clone_request,
            budget: self.budget,
        }
    }

    /// Sets the budget.
    pub fn budget(mut self, budget: impl Into<Arc<Budget>>) -> Self {
        self.budget = budget.into();
        self
    }

    /// Consume this builder and produce a [`StandardRetryPolicy`] with this
    /// builders configured settings.
    ///
    /// # Default
    ///
    /// By default, this will be set to `Budget::default()`.
    ///
    /// # Example
    ///
    /// ```
    /// # use tower::retry::standard_policy::StandardRetryPolicy;
    /// # use tower::retry::budget::Budget;
    /// // Set the Req, Res, and E type to () for simplicity, replace these with
    /// // your specific request/response/error types.
    /// StandardRetryPolicy::<(), (), ()>::builder()
    ///     .budget(Budget::default())
    ///     .build();
    /// ```
    pub fn build(self) -> StandardRetryPolicy<Req, Res, E, B> {
        let current_backoff = self.make_backoff.make_backoff();

        StandardRetryPolicy {
            is_retryable: self.is_retryable,
            clone_request: self.clone_request,
            make_backoff: self.make_backoff,
            current_backoff,
            budget: self.budget,
            _pd: PhantomData,
        }
    }
}

impl<Req, Res, E> StandardRetryPolicy<Req, Res, E, ExponentialBackoffMaker> {
    /// Create a [`StandardRetryPolicyBuilder`].
    pub fn builder() -> StandardRetryPolicyBuilder<Req, Res, E> {
        StandardRetryPolicyBuilder {
            is_retryable: Arc::new(|_: &mut Result<Res, E>| false),
            clone_request: Arc::new(|_: &Req| None),
            make_backoff: ExponentialBackoffMaker::default(),
            budget: Arc::new(Budget::default()),
        }
    }
}

impl<Req, Res, E, B> Policy<Req, Res, E> for StandardRetryPolicy<Req, Res, E, B>
where
    B: MakeBackoff,
    Req: 'static,
    Res: 'static,
    E: 'static,
{
    type Future = <B::Backoff as Backoff>::Future;

    fn retry(&mut self, _req: &mut Req, result: &mut Result<Res, E>) -> Option<Self::Future> {
        let can_retry = self.is_retryable.is_retryable(result);

        if !can_retry {
            tracing::trace!("Received non-retryable response");
            self.budget.deposit();
            return None;
        }

        let can_withdraw = self.budget.withdraw().is_ok();

        if !can_withdraw {
            tracing::trace!("Unable to withdraw from budget");
            return None;
        }

        tracing::trace!("Withdrew from retry budget");

        Some(self.current_backoff.next_backoff())
    }

    fn clone_request(&mut self, req: &Req) -> Option<Req> {
        self.clone_request.clone_request(req)
    }
}

impl<Req, Res, E, B> Clone for StandardRetryPolicy<Req, Res, E, B>
where
    B: MakeBackoff + Clone,
{
    fn clone(&self) -> Self {
        Self {
            is_retryable: self.is_retryable.clone(),
            clone_request: self.clone_request.clone(),
            budget: self.budget.clone(),
            make_backoff: self.make_backoff.clone(),
            current_backoff: self.make_backoff.make_backoff(),
            _pd: PhantomData,
        }
    }
}

impl<Req, Res, E, B: MakeBackoff + fmt::Debug> fmt::Debug for StandardRetryPolicy<Req, Res, E, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StandardRetryPolicy")
            .field("budget", &self.budget)
            .field("make_backoff", &self.make_backoff)
            .finish()
    }
}

impl<Req, Res, E, B: fmt::Debug> fmt::Debug for StandardRetryPolicyBuilder<Req, Res, E, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StandardRetryPolicyBuilder").finish()
    }
}
