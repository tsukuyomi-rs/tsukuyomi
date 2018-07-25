use futures::{Async, Future, Poll};
use http::header::HeaderValue;
use http::{header, Response};

use error::Error;
use input::{self, Input};

use super::body::ResponseBody;
use super::Output;

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

impl<T> Responder for Result<T, Error>
where
    T: Responder,
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

/// The async variant of `Responder`.
pub trait AsyncResponder: Send + 'static + sealed::Sealed {
    /// The inner type of this responder.
    type Output: Responder;

    /// Polls for a result of inner `Responder`.
    // FIXME: replace the receiver type with PinMut<Self>
    fn poll_respond_to(&mut self, input: &mut Input) -> Poll<Output, Error>;
}

impl<F> AsyncResponder for F
where
    F: Future + Send + 'static,
    F::Item: Responder,
    Error: From<F::Error>,
{
    type Output = F::Item;

    fn poll_respond_to(&mut self, input: &mut Input) -> Poll<Output, Error> {
        let x = try_ready!(input::with_set_current(input, || Future::poll(self)));
        x.respond_to(input).map(Async::Ready)
    }
}

// TODO: switch bracket impls to std::future::Future
//
//   impl<F> AsyncResponder for F
//   where
//       F: Future + Send + 'static,
//       F::Output: Responder,
//   {
//       type Output = F::Output;
//
//       fn poll_respond_to(
//           self: PinMut<Self>,
//           cx: &mut Context,
//           input: &mut Input,
//       ) -> Poll<Result<Output, Error>> {
//           let x = ready!(input::with_set_current(input, || Future::poll(self, cx)));
//           Poll::Ready(x.respond_to(input))
//       }
//   }

mod sealed {
    use futures::Future;

    use super::Responder;
    use error::Error;

    pub trait Sealed {}

    impl<F> Sealed for F
    where
        F: Future + Send + 'static,
        F::Item: Responder,
        Error: From<F::Error>,
    {
    }
}
