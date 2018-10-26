#![allow(missing_docs)]

use std::marker::PhantomData;

use bytes::Bytes;

use crate::error::Error;
use crate::input::Input;

pub trait FromInput: FromInputImpl {}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub trait FromInputImpl: Sized + 'static {
    type Error: Into<Error>;
    type Ctx;

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error>;

    fn extract(data: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error>;
}

#[derive(Debug)]
pub enum Preflight<T: FromInputImpl> {
    Completed(T),
    Partial(T::Ctx),
}

impl<T> FromInput for Option<T> where T: FromInput {}
impl<T> FromInputImpl for Option<T>
where
    T: FromInput,
{
    type Error = crate::error::Never;
    type Ctx = T::Ctx;

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match T::preflight(input) {
            Ok(Preflight::Completed(val)) => Ok(Preflight::Completed(Some(val))),
            Ok(Preflight::Partial(ctx)) => Ok(Preflight::Partial(ctx)),
            Err(..) => Ok(Preflight::Completed(None)),
        }
    }

    fn extract(body: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
        Ok(T::extract(body, input, cx).ok())
    }
}

impl FromInput for () {}
impl FromInputImpl for () {
    type Error = crate::error::Never;
    type Ctx = ();

    fn preflight(_: &mut Input<'_>) -> Result<Preflight<()>, Self::Error> {
        Ok(Preflight::Completed(()))
    }

    fn extract(_: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        unreachable!()
    }
}

impl<T> FromInput for (T,) where T: FromInput {}
impl<T> FromInputImpl for (T,)
where
    T: FromInput,
{
    type Error = T::Error;
    type Ctx = T::Ctx;

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match T::preflight(input)? {
            Preflight::Completed(val) => Ok(Preflight::Completed((val,))),
            Preflight::Partial(cx) => Ok(Preflight::Partial(cx)),
        }
    }

    fn extract(data: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
        T::extract(data, input, cx).map(|value| (value,))
    }
}

macro_rules! impl_from_input_for_tuples {
    ($($T:ident),*) => {
        impl<$($T),*> FromInput for ($($T),*)
        where
            $( $T: FromInput , )*
        {
        }

        impl<$($T),*> FromInputImpl for ($($T),*)
        where
            $( $T: FromInput , )*
        {
            type Error = Error;
            type Ctx = ($( Preflight<$T> ),*);

            #[allow(nonstandard_style)]
            fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
                $(
                    let $T = $T::preflight(input).map_err(Into::into)?;
                )*
                match ($($T),*) {
                    ($( Preflight::Completed($T) ),*) => {
                        Ok(Preflight::Completed(($($T),*)))
                    }
                    ($($T),*) => Ok(Preflight::Partial(($($T),*))),
                }
            }

            #[allow(nonstandard_style)]
            fn extract(data: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
                let ($($T),*) = cx;
                $(
                        let $T = match $T {
                        Preflight::Completed(val) => val,
                        Preflight::Partial(cx) => $T::extract(data, input, cx).map_err(Into::into)?,
                    };
                )*
                Ok(($($T),*))
            }
        }
    }
}

impl_from_input_for_tuples!(T1, T2);
impl_from_input_for_tuples!(T1, T2, T3);
impl_from_input_for_tuples!(T1, T2, T3, T4);
impl_from_input_for_tuples!(T1, T2, T3, T4, T5);
impl_from_input_for_tuples!(T1, T2, T3, T4, T5, T6);
impl_from_input_for_tuples!(T1, T2, T3, T4, T5, T6, T7);
impl_from_input_for_tuples!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_from_input_for_tuples!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_from_input_for_tuples!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);

#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
#[derive(Debug)]
pub struct State<T>(PhantomData<(fn() -> T, std::cell::UnsafeCell<()>)>);

impl<T> State<T>
where
    T: Send + Sync + 'static,
{
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        super::with_get_current(|input| {
            let state = input.state::<T>().expect("should be exist");
            f(state)
        })
    }

    pub fn clone_state(&self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }
}

impl<T> FromInput for State<T> where T: Send + Sync + 'static {}
impl<T> FromInputImpl for State<T>
where
    T: Send + Sync + 'static,
{
    type Error = Error;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Error> {
        if input.state::<T>().is_some() {
            Ok(Preflight::Completed(State(PhantomData)))
        } else {
            Err(crate::error::internal_server_error("missing state").into())
        }
    }

    fn extract(_: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Error> {
        unreachable!()
    }
}
