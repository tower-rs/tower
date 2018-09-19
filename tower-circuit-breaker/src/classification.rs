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

/// Notify the about response status.
pub trait Callback {
    fn notify(self, classification: Classification);
}

/// Classify response result.
///
/// It uses a callback mechanism through `Callback` trait to notify result of streaming responses.
pub trait ResponseClassifier<T, C: Callback> {
    type Response;

    fn classify(&self, res: T, callback: C) -> Self::Response;
}

/// A classification which produces `Classification::Failure` on each invocation.
#[derive(Debug, Copy, Clone)]
pub struct AlwaysFailure;

/// A classification which produces `Classification::Success` on each invocation.
#[derive(Debug, Copy, Clone)]
pub struct AlwaysSuccess;

impl<E> ErrorClassifier<E> for AlwaysFailure {
    fn classify(&self, _err: &E) -> Classification {
        Classification::Failure
    }
}

impl<T, C: Callback> ResponseClassifier<T, C> for AlwaysSuccess {
    type Response = T;

    fn classify(&self, res: T, callback: C) -> Self::Response {
        callback.notify(Classification::Success);
        res
    }
}
