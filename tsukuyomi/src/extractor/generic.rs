use std::sync::Arc;

// ==== Tuple/HList ====

pub trait Tuple: Sized {
    type HList: HList<Tuple = Self>;

    fn into_hlist(self) -> Self::HList;
}

impl Tuple for () {
    type HList = HNil;

    fn into_hlist(self) -> Self::HList {
        HNil(())
    }
}

pub trait HList: Sized {
    type Tuple: Tuple<HList = Self>;

    fn into_tuple(self) -> Self::Tuple;
}

#[derive(Debug)]
pub struct HNil(());

impl HList for HNil {
    type Tuple = ();

    #[inline]
    fn into_tuple(self) -> Self::Tuple {
        self.0
    }
}

#[derive(Debug)]
pub struct HCons<H, T: HList> {
    pub head: H,
    pub tail: T,
}

macro_rules! hcons {
    ($h:expr) => {
        HCons {
            head: $h,
            tail: HNil(()),
        }
    };
    ($h:expr, $($t:expr),*) => {
        HCons {
            head: $h,
            tail: hcons!($($t),*),
        }
    };
}

macro_rules! HCons {
    ($H:ty) => ( HCons<$H, HNil> );
    ($H:ty, $($T:ty),*) => ( HCons<$H, HCons!($($T),*)> );
}

macro_rules! hcons_pat {
    ($h:pat) => {
        HCons { head: $h, tail: HNil(()), }
    };
    ($h:pat, $($t:pat),*) => {
        HCons {
            head: $h,
            tail: hcons_pat!($($t),*),
        }
    };
}

macro_rules! impl_hlist {
    ($T:ident) => {
        impl<$T> Tuple for ($T,) {
            type HList = HCons!($T);

            #[inline]
            fn into_hlist(self) -> Self::HList {
                hcons!(self.0)
            }
        }

        impl<$T> HList for HCons!($T) {
            type Tuple = ($T,);

            #[inline]
            fn into_tuple(self) -> Self::Tuple {
                (self.head,)
            }
        }
    };
    ($H:ident, $($T:ident),*) => {
        impl_hlist!($($T),*);

        impl<$H, $($T),*> Tuple for ($H, $($T),*) {
            type HList = HCons!($H, $($T),*);

            #[inline]
            #[allow(non_snake_case)]
            fn into_hlist(self) -> Self::HList {
                let ($H, $($T),*) = self;
                hcons!($H, $($T),*)
            }
        }

        impl<$H, $($T),*> HList for HCons!($H, $($T),*) {
            type Tuple = ($H, $($T),*);

            #[inline]
            #[allow(non_snake_case)]
            fn into_tuple(self) -> Self::Tuple {
                let hcons_pat!($H, $($T),*) = self;
                ($H, $($T),*)
            }
        }
    };
    ($H:ident, $($T:ident,)*) => { impl_hlist!($H, $($T),*); };
}

impl_hlist! {
    T15, T14, T13, T12, T11, T10, T9, T8, T7, T6, T5, T4, T3, T2, T1, T0,
}

// ==== Combine =====

pub trait Combine<T: Tuple>: Tuple {
    type Out: Tuple;
    fn combine(self, other: T) -> Self::Out;
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<H: Tuple, T: Tuple> Combine<T> for H
where
    H::HList: CombineHList<T::HList>,
{
    type Out = <<H::HList as CombineHList<T::HList>>::Out as HList>::Tuple;

    #[inline]
    fn combine(self, other: T) -> Self::Out {
        self.into_hlist()
            .combine_hlist(other.into_hlist())
            .into_tuple()
    }
}

pub trait CombineHList<T: HList> {
    type Out: HList;

    fn combine_hlist(self, other: T) -> Self::Out;
}

impl<T: HList> CombineHList<T> for HNil {
    type Out = T;

    #[inline]
    fn combine_hlist(self, other: T) -> Self::Out {
        other
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<H, T: HList, U: HList> CombineHList<U> for HCons<H, T>
where
    T: CombineHList<U>,
    HCons<H, <T as CombineHList<U>>::Out>: HList,
{
    type Out = HCons<H, <T as CombineHList<U>>::Out>;

    #[inline]
    fn combine_hlist(self, other: U) -> Self::Out {
        HCons {
            head: self.head,
            tail: self.tail.combine_hlist(other),
        }
    }
}

// ==== Func ====

pub trait Func<Args: Tuple> {
    type Out;
    fn call(&self, args: Args) -> Self::Out;
}

impl<F, Args: Tuple> Func<Args> for Arc<F>
where
    F: Func<Args>,
{
    type Out = F::Out;

    #[inline]
    fn call(&self, args: Args) -> Self::Out {
        (**self).call(args)
    }
}

impl<F, R> Func<()> for F
where
    F: Fn() -> R,
{
    type Out = R;

    #[inline]
    fn call(&self, _: ()) -> Self::Out {
        (*self)()
    }
}

macro_rules! impl_func {
    ($T:ident) => {
        impl<F, R, $T> Func<($T,)> for F
        where
            F: Fn($T) -> R,
        {
            type Out = R;

            #[inline]
            fn call(&self, args: ($T,)) -> Self::Out {
                (*self)(args.0)
            }
        }
    };
    ($H:ident, $($T:ident),*) => {
        impl_func!($($T),*);

        impl<F, R, $H, $($T),*> Func<($H, $($T),*)> for F
        where
            F: Fn($H, $($T),*) -> R,
        {
            type Out = R;

            #[inline]
            fn call(&self, args: ($H, $($T),*)) -> Self::Out {
                #[allow(non_snake_case)]
                let ($H, $($T),*) = args;
                (*self)($H, $($T),*)
            }
        }
    };

    ($H:ident, $($T:ident,)*) => { impl_func! { $H, $($T),* } };
}

impl_func! {
    T15, T14, T13, T12, T11, T10, T9, T8, T7, T6, T5, T4, T3, T2, T1, T0,
}
