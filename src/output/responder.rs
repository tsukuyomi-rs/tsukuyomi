//! Definition of `Responder` and related components.

use futures::{Async, Future, IntoFuture, Poll};
use http::header::HeaderValue;
use http::{header, Response};
use std::fmt;

use error::Error;
use input::{self, Input};

use super::body::ResponseBody;
use super::Output;

// ==== Respond ====

/// A type representing an asynchronous computation which will be returned as a result of `Responder`.
pub struct Respond(RespondKind);

enum RespondKind {
    Ready(Option<Result<Output, Error>>),
    Async(Box<dyn FnMut(&mut Input) -> Poll<Output, Error> + Send + 'static>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Respond {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Respond").finish()
    }
}

impl Respond {
    /// Creates a `Respond` from a closure corresponding to an asynchronous computation.
    pub fn new(f: impl FnMut(&mut Input) -> Poll<Output, Error> + Send + 'static) -> Respond {
        Respond(RespondKind::Async(Box::new(f)))
    }

    #[allow(missing_docs)]
    pub fn ready(result: Result<Output, Error>) -> Respond {
        Respond(RespondKind::Ready(Some(result)))
    }

    #[allow(missing_docs)]
    pub fn wrap_future<F>(future: F) -> Respond
    where
        F: IntoFuture<Item = Output, Error = Error>,
        F::Future: Send + 'static,
    {
        let mut future = future.into_future();
        Respond::new(move |input| input::with_set_current(input, || future.poll()))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Output, Error> {
        match self.0 {
            RespondKind::Ready(ref mut result) => {
                result.take().expect("This future has already polled").map(Async::Ready)
            }
            RespondKind::Async(ref mut f) => (f)(input),
        }
    }
}

impl<F> From<F> for Respond
where
    F: IntoFuture<Item = Output, Error = Error>,
    F::Future: Send + 'static,
{
    fn from(future: F) -> Self {
        Respond::wrap_future(future)
    }
}

// ==== Responder ====

/// A trait representing the conversion to an HTTP response.
pub trait Responder {
    /// Converts `self` to an HTTP response.
    fn respond_to(self, input: &mut Input) -> Result<Output, Error>;
}

impl<T> Responder for Option<T>
where
    T: Responder,
{
    fn respond_to(self, input: &mut Input) -> Result<Output, Error> {
        self.ok_or_else(Error::not_found)?.respond_to(input)
    }
}

impl<T, E> Responder for Result<T, E>
where
    T: Responder,
    Error: From<E>,
{
    fn respond_to(self, input: &mut Input) -> Result<Output, Error> {
        self?.respond_to(input)
    }
}

impl<T> Responder for Response<T>
where
    T: Into<ResponseBody>,
{
    #[inline]
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
        Ok(self.map(Into::into))
    }
}

impl Responder for &'static str {
    #[inline]
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
        Ok(text_response(self))
    }
}

impl Responder for String {
    #[inline]
    fn respond_to(self, _: &mut Input) -> Result<Output, Error> {
        Ok(text_response(self))
    }
}

fn text_response<T: Into<ResponseBody>>(body: T) -> Output {
    let mut response = Response::new(body.into());
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

// ==== AsyncResponder ====

/// A trait representing the conversion to an HTTP response.
pub trait AsyncResponder {
    /// Converts `self` to an HTTP response.
    fn respond_to(self, input: &mut Input) -> Respond;
}

impl AsyncResponder for Respond {
    #[inline(always)]
    fn respond_to(self, _: &mut Input) -> Respond {
        self
    }
}

impl<T> AsyncResponder for T
where
    T: Responder,
{
    #[inline(always)]
    fn respond_to(self, input: &mut Input) -> Respond {
        Respond::ready(Responder::respond_to(self, input))
    }
}

// ==== IntoResponder ====

#[allow(missing_docs)]
pub trait IntoResponder {
    type Responder: AsyncResponder;

    fn into_responder(self) -> Self::Responder;
}

impl<F> IntoResponder for F
where
    F: IntoFuture,
    F::Future: Send + 'static,
    F::Item: Responder,
    Error: From<F::Error>,
{
    type Responder = AsyncResponse<F>;

    fn into_responder(self) -> Self::Responder {
        AsyncResponse(self)
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct AsyncResponse<F>(F);

impl<F> AsyncResponder for AsyncResponse<F>
where
    F: IntoFuture,
    F::Future: Send + 'static,
    F::Item: Responder,
    Error: From<F::Error>,
{
    fn respond_to(self, _: &mut Input) -> Respond {
        let mut future = self.0.into_future();
        Respond::new(move |input| {
            let item = try_ready!(input::with_set_current(input, || future.poll()));
            Responder::respond_to(item, input).map(Async::Ready)
        })
    }
}
