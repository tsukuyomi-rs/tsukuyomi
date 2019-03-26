//! Components for constructing HTTP responses.

pub mod preset;

pub use tsukuyomi_macros::Responder;

// re-export from izanami.
#[doc(no_inline)]
pub use izanami::http::{
    body::Body as ResponseBody,
    response::{IntoResponse, Response},
};

use {
    self::preset::Rendered,
    crate::{
        error::Error, //
        future::{Poll, TryFuture},
        input::Input,
        upgrade::{NeverUpgrade, Upgrade},
        util::Never,
    },
    serde::Serialize,
};

/// Create an instance of `Response<T>` with the provided body and content type.
fn make_response<T>(body: T, content_type: &'static str) -> Response
where
    T: Into<ResponseBody>,
{
    let mut response = Response::new(body.into());
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::header::HeaderValue::from_static(content_type),
    );
    response
}

/// A trait that abstracts the "reply" to the client.
///
/// # Derivation
///
/// The custom derive `Responder` is provided for reduce boilerplates around
/// trait implementations.
///
/// The macro has a parameter `#[response(preset = "..")]`, which specifies
/// the path to a type that implements a trait [`Preset`]:
///
/// ```
/// # use tsukuyomi::Responder;
/// use serde::Serialize;
///
/// #[derive(Debug, Serialize, Responder)]
/// #[response(preset = "tsukuyomi::output::preset::Json")]
/// struct Post {
///     title: String,
///     text: String,
/// }
/// # fn main() {}
/// ```
///
/// You can specify the additional trait bounds to type parameters
/// by using the parameter `#[response(bound = "..")]`:
///
/// ```
/// # use tsukuyomi::Responder;
/// # use serde::Serialize;
/// #[derive(Debug, Responder)]
/// #[response(
///     preset = "tsukuyomi::output::preset::Json",
///     bound = "T: Serialize",
///     bound = "U: Serialize",
/// )]
/// struct CustomValue<T, U> {
///     t: T,
///     u: U,
/// }
/// # fn main() {}
/// ```
///
/// [`Preset`]: ./preset/trait.Preset.html
pub trait Responder {
    /// The type of asynchronous object to be ran after upgrading the protocol.
    type Upgrade: Upgrade;

    /// The error type that will be thrown by this responder.
    type Error: Into<Error>;

    /// The asynchronous task converted from this responder.
    type Respond: Respond<Upgrade = Self::Upgrade, Error = Self::Error>;

    /// Converts itself into a `Respond`.
    fn respond(self) -> Self::Respond;
}

/// The asynchronous task that generates a reply to client.
pub trait Respond {
    type Upgrade: Upgrade;
    type Error: Into<Error>;

    fn poll_respond(
        &mut self,
        input: &mut Input<'_>,
    ) -> Poll<(Response, Option<Self::Upgrade>), Self::Error>;
}

impl<T> Respond for T
where
    T: TryFuture,
    T::Ok: IntoResponse,
{
    type Upgrade = NeverUpgrade;
    type Error = T::Error;

    fn poll_respond(
        &mut self,
        input: &mut Input<'_>,
    ) -> Poll<(Response, Option<Self::Upgrade>), Self::Error> {
        let output = futures01::try_ready!(self.poll_ready(input));
        Ok((output.into_response(), None).into())
    }
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

    impl<T> TryFuture for IntoResponseRespond<T> {
        type Ok = T;
        type Error = Never;

        #[inline]
        fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let output = self.0.take().expect("the future has already been polled.");
            Ok(output.into())
        }
    }
}

mod impl_responder_for_either {
    use {
        super::{Respond, Responder},
        crate::{error::Error, input::Input, output::Response, util::Either},
        futures01::Poll,
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

    impl<L, R> Respond for EitherRespond<L, R>
    where
        L: Respond,
        R: Respond,
    {
        type Upgrade = crate::util::Either<L::Upgrade, R::Upgrade>;
        type Error = Error;

        fn poll_respond(
            &mut self,
            input: &mut Input<'_>,
        ) -> Poll<(Response, Option<Self::Upgrade>), Self::Error> {
            match self {
                EitherRespond::Left(l) => {
                    let (res, upgrade) =
                        futures01::try_ready!(l.poll_respond(input).map_err(Into::into));
                    Ok((res, upgrade.map(crate::util::Either::Left)).into())
                }
                EitherRespond::Right(r) => {
                    let (res, upgrade) =
                        futures01::try_ready!(r.poll_respond(input).map_err(Into::into));
                    Ok((res, upgrade.map(crate::util::Either::Right)).into())
                }
            }
        }
    }
}

/// A function to create a `Responder` using the specified `TryFuture`.
pub fn respond<R, U>(respond: R) -> ResponderFn<R>
where
    R: Respond,
{
    ResponderFn(respond)
}

#[derive(Debug, Copy, Clone)]
pub struct ResponderFn<R>(R);

impl<R> Responder for ResponderFn<R>
where
    R: Respond,
{
    type Upgrade = R::Upgrade;
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

#[derive(Debug, Copy, Clone)]
pub struct Oneshot<F>(F);

mod oneshot {
    use {
        super::{Error, Input, IntoResponse, Oneshot, Responder},
        crate::future::{Poll, TryFuture},
    };

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

/// Creates a JSON responder from the specified data.
#[inline]
pub fn json<T>(data: T) -> Rendered<T, self::preset::Json>
where
    T: Serialize,
{
    Rendered::new(data)
}

/// Creates a JSON response with pretty output from the specified data.
#[inline]
pub fn json_pretty<T>(data: T) -> Rendered<T, self::preset::JsonPretty>
where
    T: Serialize,
{
    Rendered::new(data)
}

/// Creates an HTML response using the specified data.
#[inline]
pub fn html<T>(data: T) -> Rendered<T, self::preset::Html>
where
    T: Into<ResponseBody>,
{
    Rendered::new(data)
}
