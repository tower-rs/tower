use super::Layer;
use core::fmt;

/// Two [`Layer`]s chained together.
///
/// # Examples
///
/// ```rust
/// use tower_layer::{Stack, layer_fn, Layer};
///
/// let inner = layer_fn(|service| service+2);
/// let outer = layer_fn(|service| service*2);
///
/// let inner_outer_stack = Stack::new(inner, outer);
///
/// // (4 + 2) * 2 = 12
/// // (4 * 2) + 2 = 10
/// assert_eq!(inner_outer_stack.layer(4), 12);
/// ```
#[derive(Clone)]
pub struct Stack<Inner, Outer> {
    inner: Inner,
    outer: Outer,
}

impl<Inner, Outer> Stack<Inner, Outer> {
    /// Creates a new [`Stack`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tower_layer::{Stack, Identity};
    ///
    /// let stack = Stack::new(Identity::new(), Identity::new());
    /// ```
    pub const fn new(inner: Inner, outer: Outer) -> Self {
        Stack { inner, outer }
    }
}

impl<S, Inner, Outer> Layer<S> for Stack<Inner, Outer>
where
    Inner: Layer<S>,
    Outer: Layer<Inner::Service>,
{
    type Service = Outer::Service;

    fn layer(&self, service: S) -> Self::Service {
        let inner = self.inner.layer(service);

        self.outer.layer(inner)
    }
}

impl<Inner, Outer> fmt::Debug for Stack<Inner, Outer>
where
    Inner: fmt::Debug,
    Outer: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The generated output of nested `Stack`s is very noisy and makes
        // it harder to understand what is in a `ServiceBuilder`.
        //
        // Instead, this output is designed assuming that a `Stack` is
        // usually quite nested, and inside a `ServiceBuilder`. Therefore,
        // this skips using `f.debug_struct()`, since each one would force
        // a new layer of indentation.
        //
        // - In compact mode, a nested stack ends up just looking like a flat
        //   list of layers.
        //
        // - In pretty mode, while a newline is inserted between each layer,
        //   the `DebugStruct` used in the `ServiceBuilder` will inject padding
        //   to that each line is at the same indentation level.
        //
        // Also, the order of [outer, inner] is important, since it reflects
        // the order that the layers were added to the stack.
        if f.alternate() {
            // pretty
            write!(f, "{:#?},\n{:#?}", self.outer, self.inner)
        } else {
            write!(f, "{:?}, {:?}", self.outer, self.inner)
        }
    }
}
