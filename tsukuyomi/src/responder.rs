use {
    crate::{
        error::Error,
        future::{Compat01, TryFuture},
        output::IntoResponse,
        util::Never,
    },
    futures01::future::{self, FutureResult},
};

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
    type Respond = Compat01<FutureResult<Self::Response, Self::Error>>;

    #[inline]
    fn respond(self) -> Self::Respond {
        future::ok(self).into()
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
pub fn respond<R>(
    future: R,
) -> impl Responder<
    Response = R::Ok, //
    Error = R::Error,
    Respond = R,
>
where
    R: TryFuture,
    R::Ok: IntoResponse,
{
    #[allow(missing_debug_implementations)]
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

    ResponderFn(future)
}
