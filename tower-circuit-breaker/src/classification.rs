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

/// Classify response result.
///
/// It uses a callback mechanism through oneshot channel for streaming responses.
/// When response completes a classification should be sent through the channel
pub trait ResponseClassifier<T> {
    type Response;

    fn classify(&self, res: T, sender: Sender<Classification>) -> Self::Response;
}

/// A classification which produces `Classification::Failure` on each invocation.
#[derive(Debug, Copy, Clone)]
pub struct AlwaysFail;

/// Wrapper for response, which sent `Classification::Failure` through channel when dropped.
#[derive(Debug)]
pub struct FailOnDrop<T> {
    inner: T,
    channel: Option<Sender<Classification>>,
}

impl<T> FailOnDrop<T> {
    fn new(inner: T, channel: Sender<Classification>) -> Self {
        Self {
            inner,
            channel: Some(channel),
        }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}

impl<T> Drop for FailOnDrop<T> {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            let _ = channel.send(Classification::Failure);
        }
    }
}

impl<E> ErrorClassifier<E> for AlwaysFail {
    fn classify(&self, _err: &E) -> Classification {
        Classification::Failure
    }
}

impl<T> ResponseClassifier<T> for AlwaysFail {
    type Response = FailOnDrop<T>;

    fn classify(&self, res: T, sender: Sender<Classification>) -> Self::Response {
        FailOnDrop::new(res, sender)
    }
}
