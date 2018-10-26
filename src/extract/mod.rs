//! High level API for accessing request data and context information.

pub mod body;
pub mod header;
pub mod param;
pub mod query;
pub mod verb;

use std::cell::UnsafeCell;
use std::marker::PhantomData;

use bytes::Bytes;
use either::Either;

use crate::error::{Error, Never};
use crate::input::local_map::LocalData;
use crate::input::Input;

/// An enum represeting the intermediate state of `FromInput`.
#[derive(Debug)]
pub enum Preflight<T: FromInput> {
    /// Extraction has been done.
    Completed(T),

    /// Extraction is not finished yet.
    Incomplete(T::Ctx),
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<T> Preflight<T>
where
    T: FromInput,
{
    #[allow(missing_docs)]
    pub fn map_completed<U>(self, f: impl FnOnce(T) -> U) -> Preflight<U>
    where
        U: FromInput<Ctx = T::Ctx>,
    {
        match self {
            Preflight::Completed(x) => Preflight::Completed(f(x)),
            Preflight::Incomplete(cx) => Preflight::Incomplete(cx),
        }
    }
}

/// A trait representing the general data extraction from the incoming request.
pub trait FromInput: Sized + 'static {
    /// The error type which will be returned from `preflight` or `finalize`.
    type Error: Into<Error>;

    /// The type indicating intermediate state at the end of `preflight`.
    type Ctx;

    /// Attempts to extract the data before starting to receive the message body.
    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error>;

    /// Extract the data using the received message body and the intermediate state.
    ///
    /// This function will not called if `Self::preflight()` returns `Ok(Completed(x))`.
    #[allow(unused_variables)]
    fn finalize(data: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
        unreachable!("The implementation of FromInput is wrong.")
    }
}

// ---- implementors ----

impl<T> FromInput for Option<T>
where
    T: FromInput,
{
    type Error = Never;
    type Ctx = T::Ctx;

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match T::preflight(input) {
            Ok(Preflight::Completed(val)) => Ok(Preflight::Completed(Some(val))),
            Ok(Preflight::Incomplete(ctx)) => Ok(Preflight::Incomplete(ctx)),
            Err(..) => Ok(Preflight::Completed(None)),
        }
    }

    fn finalize(body: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
        Ok(T::finalize(body, input, cx).ok())
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<T> FromInput for Result<T, T::Error>
where
    T: FromInput,
{
    type Error = Never;
    type Ctx = T::Ctx;

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match T::preflight(input) {
            Ok(Preflight::Completed(val)) => Ok(Preflight::Completed(Ok(val))),
            Ok(Preflight::Incomplete(ctx)) => Ok(Preflight::Incomplete(ctx)),
            Err(err) => Ok(Preflight::Completed(Err(err))),
        }
    }

    fn finalize(body: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
        Ok(T::finalize(body, input, cx))
    }
}

impl<L, R> FromInput for Either<L, R>
where
    L: FromInput,
    R: FromInput,
{
    type Error = Error;
    type Ctx = (Option<L::Ctx>, Option<R::Ctx>);

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match L::preflight(input) {
            Ok(Preflight::Completed(x)) => Ok(Preflight::Completed(Either::Left(x))),
            Ok(Preflight::Incomplete(cx1)) => match R::preflight(input) {
                Ok(Preflight::Completed(x)) => Ok(Preflight::Completed(Either::Right(x))),
                Ok(Preflight::Incomplete(cx2)) => Ok(Preflight::Incomplete((Some(cx1), Some(cx2)))),
                Err(..) => Ok(Preflight::Incomplete((Some(cx1), None))),
            },
            Err(..) => match R::preflight(input) {
                Ok(Preflight::Completed(x)) => Ok(Preflight::Completed(Either::Right(x))),
                Ok(Preflight::Incomplete(cx)) => Ok(Preflight::Incomplete((None, Some(cx)))),
                Err(err) => Err(err.into()),
            },
        }
    }

    fn finalize(body: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
        if let Some(cx) = cx.0 {
            if let Ok(x) = L::finalize(body, input, cx) {
                return Ok(Either::Left(x));
            }
        }
        if let Some(cx) = cx.1 {
            match R::finalize(body, input, cx) {
                Ok(x) => return Ok(Either::Right(x)),
                Err(err) => return Err(err.into()),
            }
        }
        unreachable!()
    }
}

impl FromInput for () {
    type Error = Never;
    type Ctx = ();

    fn preflight(_: &mut Input<'_>) -> Result<Preflight<()>, Self::Error> {
        Ok(Preflight::Completed(()))
    }

    fn finalize(_: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        unreachable!()
    }
}

impl<T> FromInput for (T,)
where
    T: FromInput,
{
    type Error = T::Error;
    type Ctx = T::Ctx;

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match T::preflight(input)? {
            Preflight::Completed(val) => Ok(Preflight::Completed((val,))),
            Preflight::Incomplete(cx) => Ok(Preflight::Incomplete(cx)),
        }
    }

    fn finalize(data: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
        T::finalize(data, input, cx).map(|value| (value,))
    }
}

macro_rules! impl_from_input_for_tuples {
    ($($T:ident),*) => {
        impl<$($T),*> FromInput for ($($T),*)
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
                    ($($T),*) => Ok(Preflight::Incomplete(($($T),*))),
                }
            }

            #[allow(nonstandard_style)]
            fn finalize(data: &Bytes, input: &mut Input<'_>, cx: Self::Ctx) -> Result<Self, Self::Error> {
                let ($($T),*) = cx;
                $(
                        let $T = match $T {
                        Preflight::Completed(val) => val,
                        Preflight::Incomplete(cx) => $T::finalize(data, input, cx).map_err(Into::into)?,
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

// ====

impl FromInput for http::Method {
    type Error = Never;
    type Ctx = ();

    #[inline]
    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        Ok(Preflight::Completed(input.method().clone()))
    }
}

impl FromInput for http::Uri {
    type Error = Never;
    type Ctx = ();

    #[inline]
    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        Ok(Preflight::Completed(input.uri().clone()))
    }
}

impl FromInput for http::Version {
    type Error = Never;
    type Ctx = ();

    #[inline]
    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        Ok(Preflight::Completed(input.version()))
    }
}

/// A proxy object for accessing the value in the protocol extensions.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
#[derive(Debug)]
pub struct Extension<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
}

impl<T> Extension<T>
where
    T: Send + Sync + 'static,
{
    #[allow(missing_docs)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let state = input.extensions().get::<T>().expect("should be exist");
            f(state)
        })
    }
}

impl<T> FromInput for Extension<T>
where
    T: Send + Sync + 'static,
{
    type Error = Error;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Error> {
        if input.extensions().get::<T>().is_some() {
            Ok(Preflight::Completed(Self {
                _marker: PhantomData,
            }))
        } else {
            Err(crate::error::internal_server_error("missing extension").into())
        }
    }
}

/// A proxy object for accessing the global state.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
#[derive(Debug)]
pub struct State<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
}

impl<T> State<T>
where
    T: Send + Sync + 'static,
{
    #[allow(missing_docs)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let state = input.state::<T>().expect("should be exist");
            f(state)
        })
    }
}

impl<T> FromInput for State<T>
where
    T: Send + Sync + 'static,
{
    type Error = Error;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        if input.state::<T>().is_some() {
            Ok(Preflight::Completed(Self {
                _marker: PhantomData,
            }))
        } else {
            Err(crate::error::internal_server_error("missing state").into())
        }
    }
}

// ==== Local ====

/// A proxy object for accessing local data from handler functions.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
#[derive(Debug)]
pub struct Local<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
}

#[allow(missing_docs)]
impl<T> Local<T>
where
    T: LocalData,
{
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let value = T::get(input.locals()).expect("should be Some");
            f(value)
        })
    }

    pub fn with_mut<R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let value = T::get_mut(input.locals_mut()).expect("should be Some");
            f(value)
        })
    }
}

impl<T> FromInput for Local<T>
where
    T: LocalData,
{
    type Error = Error;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        if input.locals().contains_key(&T::KEY) {
            Ok(Preflight::Completed(Self {
                _marker: PhantomData,
            }))
        } else {
            Err(crate::error::internal_server_error("missing local value").into())
        }
    }
}
