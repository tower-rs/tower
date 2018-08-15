#[macro_use]
macro_rules! kind_error {
    ($(#[$m:meta])* pub struct $name:ident from enum $kind_ty:ident {
            $(
                $(#[$vmeta:meta])*
                $variant:ident$(($t:ident))* =>
                    $(fmt: $fmt:expr,)* is: $is:ident, into: $into:ident, borrow: $borrow:ident
            ),+
    }) => {
        $(#[$m])*
        pub struct $name<$($($t,)*)+> {
            kind: $kind_ty<$($($t,)*)+>,
        }

        $(#[$m])*
        enum $kind_ty<$($($t,)*)+> {
            $(
                $(#[$vmeta])*
                $variant$(($t))*
            ),+
        }

        impl<$($($t,)*)+> $crate::std::fmt::Display for $name<$($($t,)*)+>
        where $( $($t: $crate::std::fmt::Display,)* )+
        {
            fn fmt(&self, f: &mut $crate::std::fmt::Formatter) -> $crate::std::fmt::Result {
                match self.kind {
                    $(
                        $kind_ty::$variant$((ref e@$t))* => arm!( fmt: $kind_ty::$variant$((e@$t))*, $( $fmt, )* f ),
                    )+
                }

            }
        }

        impl<$($($t,)*)+> $crate::std::error::Error for $name<$($($t,)*)+>
        where $( $($t: $crate::std::error::Error + $crate::std::fmt::Display,)* )+
        {
            fn cause(&self) -> Option<&$crate::std::error::Error> {
                match self.kind {
                   $(
                        $kind_ty::$variant$((ref e@$t))* => arm!( cause: $kind_ty::$variant$((e@$t))* ),
                   )+
                }
            }
        }

        impl<$($($t,)*)+> From<$kind_ty<$($($t,)*)+>> for $name<$($($t,)*)+> {
            fn from(kind: $kind_ty<$($($t,)*)+>) -> Self {
                Self { kind, }
            }
        }

        impl<$($($t,)*)+> $name<$($($t,)*)+> {
            $(
                pub fn $is(&self) -> bool {
                    match self.kind {
                        $kind_ty::$variant$((ref _e @$t))* => true,
                        _ => false,
                    }
                }

                $(
                    pub fn $into(self) -> Option<$t> {
                        match self.kind {
                            $kind_ty::$variant(e) => Some(e),
                            _ => None,
                        }
                    }

                    pub fn $borrow(&self) -> Option<&$t> {
                        match self.kind {
                            $kind_ty::$variant(ref e) => Some(e),
                            _ => None,
                        }
                    }
                )*
            )+
        }
    };
}

macro_rules! arm {
    (cause: $name:ident::$variant:ident($e:ident@$ty:ident)) => {
        $e.cause().or(Some($e))
    };
    (cause: $name:ident::$variant:ident) => {
        { None }
    };

    (fmt: $name:ident::$variant:ident($e:ident@$ty:ident), $fmt:expr, $f:expr) => {
        { write!($f, concat!($fmt, ": ", "{}"), $e) }
    };
    (fmt: $name:ident::$variant:ident($e:ident@$ty:ident), $f:expr) => {
        { $crate::std::fmt::Display::fmt($e, $f) }
    };
    (fmt: $name:ident::$variant:ident, $fmt:expr, $f:expr) => {
        { $f.pad($fmt) }
    };
}
