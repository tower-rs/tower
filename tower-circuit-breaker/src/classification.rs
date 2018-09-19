use futures::sync::oneshot::Sender;

/// Error/complete classification, it indicates whether or not a request ends with an error.
#[derive(Debug)]
pub enum Classification {
    /// Request failed.
    Failure,
    /// Request was successful
    Success,
}

/// Classify response error.
pub trait ErrorClassifier<E> {
    fn classify(&self, err: &E) -> Classification;
}

/// A classification which produces `Classification::Failure` on each invocation.
#[derive(Debug, Copy, Clone)]
pub struct AlwaysFailure;

/// A classification which produces `Classification::Success` on each invocation.
#[derive(Debug, Copy, Clone)]
pub struct AlwaysSuccess;

/// Classify response result.
///
/// It uses a callback mechanism through oneshot channel for streaming responses.
/// When response completes a classification should be sent through the channel
pub trait ResponseClassifier<T> {
    type Response;

    fn classify(&self, res: T, sender: Sender<Classification>) -> Self::Response;
}

impl<E> ErrorClassifier<E> for AlwaysFailure {
    fn classify(&self, _err: &E) -> Classification {
        Classification::Failure
    }
}

impl<T> ResponseClassifier<T> for AlwaysSuccess {
    type Response = T;

    fn classify(&self, res: T, sender: Sender<Classification>) -> Self::Response {
        let _ = sender.send(Classification::Success);
        res
    }
}
