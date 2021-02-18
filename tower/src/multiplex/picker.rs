/// TODO(david): docs
pub trait Picker<R: ?Sized> {
    // `&mut R` so we can insert into request extensions while picking a service. Useful for
    // parsing the URI
    /// TODO(david): docs
    fn pick(&mut self, req: &mut R) -> Pick;

    /// Combine two `Picker`s into one. Will return `Pick::First` if _both_ returns `Pick::First`,
    /// otherwise `Pick::Second`.
    fn and<P>(self, next: P) -> And<Self, P>
    where
        Self: Sized,
        P: Picker<R>,
    {
        And {
            left: self,
            right: next,
        }
    }

    /// Combine two `Picker`s into one. Will return `Pick::First` if _either_ returns `Pick::First`,
    /// otherwise `Pick::Second`.
    fn or<P>(self, next: P) -> Or<Self, P>
    where
        Self: Sized,
        P: Picker<R>,
    {
        Or {
            left: self,
            right: next,
        }
    }
}

impl<F, T> Picker<T> for F
where
    F: FnMut(&mut T) -> Pick,
{
    fn pick(&mut self, req: &mut T) -> Pick {
        self(req)
    }
}

/// TODO(david): docs
#[derive(Debug, Copy, Clone)]
pub enum Pick {
    /// TODO(david): docs
    First,
    /// TODO(david): docs
    Second,
}

/// TODO(david): docs
#[derive(Debug, Copy, Clone)]
pub struct And<A, B> {
    left: A,
    right: B,
}

impl<R, A, B> Picker<R> for And<A, B>
where
    A: Picker<R>,
    B: Picker<R>,
{
    fn pick(&mut self, req: &mut R) -> Pick {
        let left_pick = self.left.pick(req);
        let right_pick = self.right.pick(req);

        if let (Pick::First, Pick::First) = (left_pick, right_pick) {
            Pick::First
        } else {
            Pick::Second
        }
    }
}

/// TODO(david): docs
#[derive(Debug, Copy, Clone)]
pub struct Or<A, B> {
    left: A,
    right: B,
}

impl<R, A, B> Picker<R> for Or<A, B>
where
    A: Picker<R>,
    B: Picker<R>,
{
    fn pick(&mut self, req: &mut R) -> Pick {
        let left_pick = self.left.pick(req);
        let right_pick = self.right.pick(req);

        match (left_pick, right_pick) {
            (Pick::First, _) | (_, Pick::First) => Pick::First,
            _ => Pick::Second,
        }
    }
}

/// TODO(david): docs
pub fn not<P>(picker: P) -> Not<P> {
    Not(picker)
}

/// TODO(david): docs
#[derive(Debug, Copy, Clone)]
pub struct Not<P>(P);

impl<P, R> Picker<R> for Not<P>
where
    P: Picker<R>,
{
    fn pick(&mut self, req: &mut R) -> Pick {
        match self.0.pick(req) {
            Pick::First => Pick::Second,
            Pick::Second => Pick::First,
        }
    }
}
