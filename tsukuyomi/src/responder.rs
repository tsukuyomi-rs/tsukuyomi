use {
    crate::{error::Error, input::Input, output::IntoResponse, util::Never},
    futures01::future::{self, Future, FutureResult, IntoFuture},
};

/// A trait representing a reply to the client.
pub trait Responder {
    /// The type of response
    type Response: IntoResponse;

    /// The error type which will be returned from `respond_to`.
    type Error: Into<Error>;

    /// The type of `Future` which will be returned from `respond_to`.
    type Future: Future<Item = Self::Response, Error = Self::Error>;

    /// Converts itself into a `Future` that will be resolved as a `Response`.
    fn respond(self, input: &mut Input<'_>) -> Self::Future;
}

/// a branket impl of `Responder` for `IntoResponse`s.
impl<T> Responder for T
where
    T: IntoResponse,
{
    type Response = T;
    type Error = Never;
    type Future = FutureResult<Self::Response, Self::Error>;

    #[inline]
    fn respond(self, _: &mut Input<'_>) -> Self::Future {
        future::ok(self)
    }
}

mod impl_responder_for_either {
    use {
        super::{IntoResponse, Responder},
        crate::{error::Error, input::Input, util::Either},
        futures01::{Future, Poll},
    };

    impl<L, R> Responder for Either<L, R>
    where
        L: Responder,
        R: Responder,
    {
        type Response = either::Either<L::Response, R::Response>;
        type Error = Error;
        type Future = EitherFuture<L::Future, R::Future>;

        fn respond(self, input: &mut Input<'_>) -> Self::Future {
            match self {
                Either::Left(l) => EitherFuture::Left(l.respond(input)),
                Either::Right(r) => EitherFuture::Right(r.respond(input)),
            }
        }
    }

    #[allow(missing_debug_implementations)]
    pub enum EitherFuture<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R> Future for EitherFuture<L, R>
    where
        L: Future,
        R: Future,
        L::Error: Into<Error>,
        R::Error: Into<Error>,
        L::Item: IntoResponse,
        R::Item: IntoResponse,
    {
        type Item = either::Either<L::Item, R::Item>;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match self {
                EitherFuture::Left(l) => l
                    .poll()
                    .map(|x| x.map(either::Either::Left))
                    .map_err(Into::into),
                EitherFuture::Right(r) => r
                    .poll()
                    .map(|x| x.map(either::Either::Right))
                    .map_err(Into::into),
            }
        }
    }
}

/// A function to create a `Responder` using the specified function.
pub fn respond<R>(
    f: impl FnOnce(&mut Input<'_>) -> R,
) -> impl Responder<
    Response = R::Item, //
    Error = R::Error,
    Future = R::Future,
>
where
    R: IntoFuture,
    R::Item: IntoResponse,
    R::Error: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    pub struct ResponderFn<F>(F);

    impl<F, R> Responder for ResponderFn<F>
    where
        F: FnOnce(&mut Input<'_>) -> R,
        R: IntoFuture,
        R::Item: IntoResponse,
        R::Error: Into<Error>,
    {
        type Response = R::Item;
        type Error = R::Error;
        type Future = R::Future;

        #[inline]
        fn respond(self, input: &mut Input<'_>) -> Self::Future {
            (self.0)(input).into_future()
        }
    }

    ResponderFn(f)
}
