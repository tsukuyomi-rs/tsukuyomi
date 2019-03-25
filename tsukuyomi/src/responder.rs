//! Definition of `Responder`.

use {
    crate::{
        error::Error, //
        future::TryFuture,
        input::Input,
        output::{body::ResponseBody, IntoResponse},
        upgrade::Upgrade,
    },
    http::Response,
};

pub use self::oneshot::Oneshot;

/// A trait that abstracts asynchronous tasks involving a reply to the client.
pub trait Responder {
    /// The type of asynchronous object to be ran after upgrading the protocol.
    type Upgrade: Upgrade;

    /// The error type that will be thrown by this responder.
    type Error: Into<Error>;

    /// The `TryFuture` that represents the actual process of this responder.
    type Respond: TryFuture<
        Ok = (Response<ResponseBody>, Option<Self::Upgrade>), //
        Error = Self::Error,
    >;

    /// Converts itself into a `Respond`.
    fn respond(self) -> Self::Respond;
}

/// a branket impl of `Responder` for `IntoResponse`s.
impl<T> Responder for T
where
    T: IntoResponse,
{
    type Upgrade = crate::upgrade::NeverUpgrade;
    type Error = T::Error;
    type Respond = self::impl_responder_for_T::IntoResponseRespond<T>;

    #[inline]
    fn respond(self) -> Self::Respond {
        self::impl_responder_for_T::IntoResponseRespond(Some(self))
    }
}

#[allow(nonstandard_style)]
mod impl_responder_for_T {
    use super::*;

    #[allow(missing_debug_implementations)]
    pub struct IntoResponseRespond<T>(pub(super) Option<T>);

    impl<T> TryFuture for IntoResponseRespond<T>
    where
        T: IntoResponse,
    {
        type Ok = (Response<ResponseBody>, Option<crate::upgrade::NeverUpgrade>);
        type Error = T::Error;

        #[inline]
        fn poll_ready(
            &mut self,
            input: &mut Input<'_>,
        ) -> crate::future::Poll<Self::Ok, Self::Error> {
            let output = self.0.take().expect("the future has already been polled.");
            let response = output.into_response(input.request)?.map(Into::into);
            Ok((response, None).into())
        }
    }
}

mod impl_responder_for_either {
    use {
        super::Responder,
        crate::{
            error::Error,
            future::{Poll, TryFuture},
            input::Input,
            output::body::ResponseBody,
            upgrade::Upgrade,
            util::Either,
        },
        http::Response,
    };

    impl<L, R> Responder for Either<L, R>
    where
        L: Responder,
        R: Responder,
    {
        type Upgrade = crate::util::Either<L::Upgrade, R::Upgrade>;
        type Error = Error;
        type Respond = EitherRespond<L::Respond, R::Respond>;

        fn respond(self) -> Self::Respond {
            match self {
                Either::Left(l) => EitherRespond::Left(l.respond()),
                Either::Right(r) => EitherRespond::Right(r.respond()),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub enum EitherRespond<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R, LU, RU> TryFuture for EitherRespond<L, R>
    where
        L: TryFuture<Ok = (Response<ResponseBody>, Option<LU>)>,
        R: TryFuture<Ok = (Response<ResponseBody>, Option<RU>)>,
        LU: Upgrade,
        RU: Upgrade,
    {
        type Ok = (Response<ResponseBody>, Option<crate::util::Either<LU, RU>>);
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            match self {
                EitherRespond::Left(l) => {
                    let (res, upgrade) =
                        futures01::try_ready!(l.poll_ready(input).map_err(Into::into));
                    Ok((res, upgrade.map(crate::util::Either::Left)).into())
                }
                EitherRespond::Right(r) => {
                    let (res, upgrade) =
                        futures01::try_ready!(r.poll_ready(input).map_err(Into::into));
                    Ok((res, upgrade.map(crate::util::Either::Right)).into())
                }
            }
        }
    }
}

/// A function to create a `Responder` using the specified `TryFuture`.
pub fn respond<R, U>(future: R) -> ResponderFn<R>
where
    R: TryFuture<Ok = (Response<ResponseBody>, Option<U>)>,
{
    ResponderFn(future)
}

#[derive(Debug, Copy, Clone)]
pub struct ResponderFn<R>(R);

impl<R, U> Responder for ResponderFn<R>
where
    R: TryFuture<Ok = (Response<ResponseBody>, Option<U>)>,
    U: Upgrade,
{
    type Upgrade = U;
    type Error = R::Error;
    type Respond = R;

    #[inline]
    fn respond(self) -> Self::Respond {
        self.0
    }
}

/// Creates a `Responder` from a function that returns its result immediately.
///
/// The passed function can access the request context once when called.
pub fn oneshot<F, T, E>(f: F) -> Oneshot<F>
where
    F: FnOnce(&mut Input<'_>) -> Result<T, E>,
    T: IntoResponse,
    E: Into<Error>,
{
    Oneshot(f)
}

mod oneshot {
    use {
        super::{Error, Input, IntoResponse, Responder, Response, ResponseBody},
        crate::future::{Poll, TryFuture},
    };

    #[derive(Debug, Copy, Clone)]
    pub struct Oneshot<F>(pub(super) F);

    impl<F, T, E> Responder for Oneshot<F>
    where
        F: FnOnce(&mut Input<'_>) -> Result<T, E>,
        T: IntoResponse,
        E: Into<Error>,
    {
        type Upgrade = crate::upgrade::NeverUpgrade;
        type Error = Error;
        type Respond = OneshotRespond<F>;

        #[inline]
        fn respond(self) -> Self::Respond {
            OneshotRespond(Some(self.0))
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct OneshotRespond<F>(Option<F>);

    impl<F, T, E> TryFuture for OneshotRespond<F>
    where
        F: FnOnce(&mut Input<'_>) -> Result<T, E>,
        T: IntoResponse,
        E: Into<Error>,
    {
        type Ok = (Response<ResponseBody>, Option<crate::upgrade::NeverUpgrade>);
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let f = self.0.take().expect("the future has already polled");
            let output = f(input).map_err(Into::into)?;
            let response = output
                .into_response(input.request)
                .map_err(Into::into)?
                .map(Into::into);
            Ok((response, None).into())
        }
    }
}
