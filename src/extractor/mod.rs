//! High level API for accessing request data and context information.

#![allow(missing_docs)]

mod from_input;

pub mod body;
pub mod header;
pub mod param;
pub mod query;
pub mod verb;

pub use self::from_input::{Directly, Extension, FromInput, Local, State};

// ==== impl ====

use std::fmt;

use bytes::Bytes;
use derive_more::From;
use either::Either;
use futures::Future;

use crate::error::{Error, Never};
use crate::input::Input;

pub trait Extractor {
    type Out;
    type Ctx;
    type Error: Into<Error>;

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error>;

    #[allow(unused_variables)]
    fn finalize(
        ctx: Self::Ctx,
        input: &mut Input<'_>,
        body: &Bytes,
    ) -> Result<Self::Out, Self::Error> {
        unreachable!("The implementation of Extractor is wrong.")
    }
}

#[derive(Debug, Default, From)]
pub struct Optional<E>(E);

impl<E> Extractor for Optional<E>
where
    E: Extractor,
{
    type Out = Option<E::Out>;
    type Ctx = E::Ctx;
    type Error = Never;

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match self.0.preflight(input) {
            Ok(preflight) => Ok(preflight.map_completed(Some)),
            Err(..) => Ok(Preflight::Completed(None)),
        }
    }

    fn finalize(
        cx: Self::Ctx,
        input: &mut Input<'_>,
        body: &Bytes,
    ) -> Result<Self::Out, Self::Error> {
        Ok(E::finalize(cx, input, body).ok())
    }
}

#[derive(Debug, Default, From)]
pub struct Fallible<E>(E);

impl<E> Extractor for Fallible<E>
where
    E: Extractor,
{
    type Out = Result<E::Out, E::Error>;
    type Ctx = E::Ctx;
    type Error = Never;

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match self.0.preflight(input) {
            Ok(preflight) => Ok(preflight.map_completed(Ok)),
            Err(err) => Ok(Preflight::Completed(Err(err))),
        }
    }

    fn finalize(
        cx: Self::Ctx,
        input: &mut Input<'_>,
        body: &Bytes,
    ) -> Result<Self::Out, Self::Error> {
        Ok(E::finalize(cx, input, body))
    }
}

#[derive(Debug)]
pub struct EitherOf<L, R> {
    left: L,
    right: R,
}

impl<L, R> EitherOf<L, R>
where
    L: Extractor,
    R: Extractor,
{
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L, R> Extractor for EitherOf<L, R>
where
    L: Extractor,
    R: Extractor,
{
    type Out = Either<L::Out, R::Out>;
    type Ctx = (Option<L::Ctx>, Option<R::Ctx>);
    type Error = Error;

    fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        match self.left.preflight(input) {
            Ok(Preflight::Completed(x)) => Ok(Preflight::Completed(Either::Left(x))),
            Ok(Preflight::Incomplete(cx1)) => match self.right.preflight(input) {
                Ok(Preflight::Completed(x)) => Ok(Preflight::Completed(Either::Right(x))),
                Ok(Preflight::Incomplete(cx2)) => Ok(Preflight::Incomplete((Some(cx1), Some(cx2)))),
                Err(..) => Ok(Preflight::Incomplete((Some(cx1), None))),
            },
            Err(..) => match self.right.preflight(input) {
                Ok(Preflight::Completed(x)) => Ok(Preflight::Completed(Either::Right(x))),
                Ok(Preflight::Incomplete(cx)) => Ok(Preflight::Incomplete((None, Some(cx)))),
                Err(err) => Err(err.into()),
            },
        }
    }

    fn finalize(
        cx: Self::Ctx,
        input: &mut Input<'_>,
        data: &Bytes,
    ) -> Result<Self::Out, Self::Error> {
        if let Some(cx) = cx.0 {
            if let Ok(x) = L::finalize(cx, input, data) {
                return Ok(Either::Left(x));
            }
        }

        if let Some(cx) = cx.1 {
            match R::finalize(cx, input, data) {
                Ok(x) => return Ok(Either::Right(x)),
                Err(err) => return Err(err.into()),
            }
        }

        unreachable!()
    }
}

mod tuple {
    use super::*;

    impl Extractor for () {
        type Out = ();
        type Error = Never;
        type Ctx = ();

        #[inline]
        fn preflight(&self, _: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
            Ok(Preflight::Completed(()))
        }
    }

    macro_rules! impl_extractor_for_tuples {
        ($H:ident, $($T:ident),*) => {
            impl_extractor_for_tuples!($($T),*);

            impl<$H, $($T),*> Extractor for ($H, $($T),*)
            where
                   $H: Extractor,
                $( $T: Extractor, )*
            {
                type Out = ($H::Out, $($T::Out),*);
                type Ctx = (Preflight<$H>, $(Preflight<$T>),*);
                type Error = Error;

                #[allow(nonstandard_style)]
                fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
                    let (ref $H, $(ref $T),*) = self;
                    let $H = $H.preflight(input).map_err(Into::into)?;
                    $(
                        let $T = $T.preflight(input).map_err(Into::into)?;
                    )*
                    match ($H, $($T),*) {
                        (Preflight::Completed($H), $( Preflight::Completed($T) ),*) => {
                            Ok(Preflight::Completed(($H, $($T),*)))
                        }
                        ($H, $($T),*) => Ok(Preflight::Incomplete(($H, $($T),*))),
                    }
                }

                #[allow(nonstandard_style)]
                fn finalize(cx: Self::Ctx, input: &mut Input<'_>, data: &Bytes) -> Result<Self::Out, Self::Error> {
                    let ($H, $($T),*) = cx;
                    let $H = match $H {
                        Preflight::Completed(val) => val,
                        Preflight::Incomplete(cx) => $H::finalize(cx, input, data).map_err(Into::into)?,
                    };
                    $(
                        let $T = match $T {
                            Preflight::Completed(val) => val,
                            Preflight::Incomplete(cx) => $T::finalize(cx, input, data).map_err(Into::into)?,
                        };
                    )*
                    Ok(($H, $($T),*))
                }
            }
        };
        ($E:ident) => {
            impl<$E> Extractor for ($E,)
            where
                $E: Extractor,
            {
                type Out = ($E::Out,);
                type Ctx = $E::Ctx;
                type Error = $E::Error;

                #[inline]
                fn preflight(&self, input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
                    self.0
                        .preflight(input)
                        .map(|pre| pre.map_completed(|out| (out,)))
                }

                #[inline]
                fn finalize(
                    cx: Self::Ctx,
                    input: &mut Input<'_>,
                    data: &Bytes,
                ) -> Result<Self::Out, Self::Error> {
                    $E::finalize(cx, input, data).map(|out| (out,))
                }
            }
        };
    }

    impl_extractor_for_tuples!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
}

// ==== Preflight ====

pub enum Preflight<E: Extractor + ?Sized> {
    Completed(E::Out),
    Incomplete(E::Ctx),
}

impl<E> fmt::Debug for Preflight<E>
where
    E: Extractor + ?Sized,
    E::Out: fmt::Debug,
    E::Ctx: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Preflight::Completed(ref out) => f.debug_tuple("Completed").field(out).finish(),
            Preflight::Incomplete(ref cx) => f.debug_tuple("Incomplete").field(cx).finish(),
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<E> Preflight<E>
where
    E: Extractor + ?Sized,
{
    #[allow(missing_docs)]
    pub fn map_completed<U>(self, f: impl FnOnce(E::Out) -> U::Out) -> Preflight<U>
    where
        U: Extractor<Ctx = E::Ctx> + ?Sized,
    {
        match self {
            Preflight::Completed(x) => Preflight::Completed(f(x)),
            Preflight::Incomplete(cx) => Preflight::Incomplete(cx),
        }
    }

    #[allow(missing_docs)]
    pub fn map_incomplete<U>(self, f: impl FnOnce(E::Ctx) -> U::Ctx) -> Preflight<U>
    where
        U: Extractor<Out = E::Out> + ?Sized,
    {
        match self {
            Preflight::Completed(x) => Preflight::Completed(x),
            Preflight::Incomplete(cx) => Preflight::Incomplete(f(cx)),
        }
    }

    #[allow(missing_docs)]
    pub fn conform<U>(self) -> Preflight<U>
    where
        U: Extractor<Out = E::Out, Ctx = E::Ctx> + ?Sized,
    {
        match self {
            Preflight::Completed(out) => Preflight::Completed(out),
            Preflight::Incomplete(cx) => Preflight::Incomplete(cx),
        }
    }
}

// ==== extract ====

pub(crate) fn extract<T>(
    extractor: &T,
    input: &mut Input<'_>,
) -> impl Future<Item = T::Out, Error = Error>
where
    T: Extractor + ?Sized,
{
    use futures::future::{err, ok, Either};
    match extractor.preflight(input) {
        Ok(Preflight::Completed(data)) => Either::A(ok(data)),
        Err(preflight_err) => Either::A(err(preflight_err.into())),
        Ok(Preflight::Incomplete(cx)) => Either::B(
            input
                .body_mut()
                .read_all()
                .map_err(Error::critical)
                .and_then(move |data| {
                    crate::input::with_get_current(|input| T::finalize(cx, input, &data))
                        .map_err(Into::into)
                }),
        ),
    }
}
