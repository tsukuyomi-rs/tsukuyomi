//! Definition of `Responder`.

use crate::{error::Error, future::TryFuture, input::Input, output::IntoResponse, util::Never};

pub use self::oneshot::Oneshot;

/// A trait that abstracts replies to clients.
pub trait Responder {
    /// The type of response
    type Response: IntoResponse;

    /// The error type which will be returned from `respond_to`.
    type Error: Into<Error>;

    /// The type of `Future` which will be returned from `respond_to`.
    type Respond: TryFuture<Ok = Self::Response, Error = Self::Error>;

    /// Converts itself into a `TryFuture` that will be resolved as a `Response`.
    fn respond(self) -> Self::Respond;
}

/// a branket impl of `Responder` for `IntoResponse`s.
impl<T> Responder for T
where
    T: IntoResponse,
{
    type Response = T;
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

    impl<T> TryFuture for IntoResponseRespond<T> {
        type Ok = T;
        type Error = Never;

        #[inline]
        fn poll_ready(&mut self, _: &mut Input<'_>) -> crate::future::Poll<Self::Ok, Self::Error> {
            let output = self.0.take().expect("the future has already been polled.");
            Ok(output.into())
        }
    }
}

mod impl_responder_for_either {
    use {
        super::{IntoResponse, Responder},
        crate::{
            error::Error,
            future::{Poll, TryFuture},
            input::Input,
            util::Either,
        },
    };

    impl<L, R> Responder for Either<L, R>
    where
        L: Responder,
        R: Responder,
    {
        type Response = either::Either<L::Response, R::Response>;
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

    impl<L, R> TryFuture for EitherRespond<L, R>
    where
        L: TryFuture,
        R: TryFuture,
        L::Ok: IntoResponse,
        R::Ok: IntoResponse,
    {
        type Ok = either::Either<L::Ok, R::Ok>;
        type Error = Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            match self {
                EitherRespond::Left(l) => l
                    .poll_ready(input)
                    .map(|x| x.map(either::Either::Left))
                    .map_err(Into::into),
                EitherRespond::Right(r) => r
                    .poll_ready(input)
                    .map(|x| x.map(either::Either::Right))
                    .map_err(Into::into),
            }
        }
    }
}

/// A function to create a `Responder` using the specified `TryFuture`.
pub fn respond<R>(future: R) -> ResponderFn<R>
where
    R: TryFuture,
    R::Ok: IntoResponse,
{
    ResponderFn(future)
}

#[derive(Debug, Copy, Clone)]
pub struct ResponderFn<R>(R);

impl<R> Responder for ResponderFn<R>
where
    R: TryFuture,
    R::Ok: IntoResponse,
{
    type Response = R::Ok;
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
        type Response = T;
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
        E: Into<Error>,
    {
        type Ok = T;
        type Error = E;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let f = self.0.take().expect("the future has already polled");
            f(input).map(Into::into)
        }
    }
}
