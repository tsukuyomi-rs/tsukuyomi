//! Definition of `Responder`.

use {
    crate::{
        error::Error, //
        future::TryFuture,
        input::Input,
        output::{body::ResponseBody, IntoResponse},
        upgrade::Upgrade,
        util::Never,
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
    type Error = Never;
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
        type Error = Never;

        #[inline]
        fn poll_ready(&mut self, _: &mut Input<'_>) -> crate::future::Poll<Self::Ok, Self::Error> {
            let output = self.0.take().expect("the future has already been polled.");
            Ok((output.into_response(), None).into())
        }
    }
}

impl<T> Responder for Option<T>
where
    T: Responder,
{
    type Upgrade = T::Upgrade;
    type Error = Error;
    type Respond = self::impl_responder_for_option::OptionRespond<T::Respond>;

    fn respond(self) -> Self::Respond {
        self::impl_responder_for_option::OptionRespond(self.map(Responder::respond))
    }
}

#[allow(nonstandard_style)]
mod impl_responder_for_option {
    use super::*;

    #[allow(missing_debug_implementations)]
    pub struct OptionRespond<F>(pub(super) Option<F>);

    impl<F, U> TryFuture for OptionRespond<F>
    where
        F: TryFuture<Ok = (Response<ResponseBody>, Option<U>)>,
        F::Error: Into<Error>,
        U: Upgrade,
    {
        type Ok = (Response<ResponseBody>, Option<U>);
        type Error = Error;

        #[inline]
        fn poll_ready(
            &mut self,
            input: &mut Input<'_>,
        ) -> crate::future::Poll<Self::Ok, Self::Error> {
            match self.0 {
                Some(ref mut fut) => fut.poll_ready(input).map_err(Into::into),
                None => Err(crate::error::not_found("None")),
            }
        }
    }
}

impl<T, E> Responder for Result<T, E>
where
    T: Responder,
    E: Into<Error>,
{
    type Upgrade = T::Upgrade;
    type Error = Error;
    type Respond = self::impl_responder_for_result::ResultRespond<T::Respond, E>;

    fn respond(self) -> Self::Respond {
        self::impl_responder_for_result::ResultRespond(self.map(Responder::respond).map_err(Some))
    }
}

#[allow(nonstandard_style)]
mod impl_responder_for_result {
    use super::*;

    #[allow(missing_debug_implementations)]
    pub struct ResultRespond<F, E>(pub(super) Result<F, Option<E>>);

    impl<F, E, U> TryFuture for ResultRespond<F, E>
    where
        F: TryFuture<Ok = (Response<ResponseBody>, Option<U>)>,
        F::Error: Into<Error>,
        U: Upgrade,
        E: Into<Error>,
    {
        type Ok = (Response<ResponseBody>, Option<U>);
        type Error = Error;

        #[inline]
        fn poll_ready(
            &mut self,
            input: &mut Input<'_>,
        ) -> crate::future::Poll<Self::Ok, Self::Error> {
            match self.0 {
                Ok(ref mut fut) => fut.poll_ready(input).map_err(Into::into),
                Err(ref mut e) => Err(e
                    .take()
                    .expect("the future has already been polled.")
                    .into()),
            }
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
        super::{Error, Input, IntoResponse, Responder},
        crate::{
            future::{Poll, TryFuture},
            output::body::ResponseBody,
        },
        http::Response,
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
        type Error = E;
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
        type Error = E;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let f = self.0.take().expect("the future has already polled");
            f(input).map(|res| (res.into_response(), None).into())
        }
    }
}
