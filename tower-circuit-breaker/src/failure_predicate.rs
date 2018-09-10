/// Evaluates if an error should be recorded as a failure and thus increase the failure rate.
pub trait FailurePredicate<E> {
    /// Must return `true` if the error should count as a failure, otherwise it must return `false`.
    fn is_err(&self, err: &E) -> bool;
}

impl<F, E> FailurePredicate<E> for F
where
    F: Fn(&E) -> bool,
{
    #[inline]
    fn is_err(&self, err: &E) -> bool {
        self(err)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Any;

impl<E> FailurePredicate<E> for Any {
    #[inline]
    fn is_err(&self, _err: &E) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_func_as_failure_predicate() {
        fn is_err(err: &bool) -> bool {
            *err
        }
        assert!(FailurePredicate::is_err(&is_err, &true));
    }
}
