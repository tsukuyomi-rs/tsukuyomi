//! Components for receiving incoming request bodies.

use bytes::{Bytes, BytesMut};
use futures::{Async, Future, IntoFuture, Poll};
use mime;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use std::{fmt, mem};

use crate::error::{Error, Failure};
use crate::server::rt;
use crate::server::service::http::{Payload as _Payload, RequestBody as RawBody, UpgradedIo};
use crate::server::CritError;

use super::from_input::{FromInput, FromInputImpl, Preflight};
use super::global::with_get_current;
use super::header::content_type;
use super::Input;

// ==== RequestBody ====

/// A type representing a message body in the incoming HTTP request.
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
#[derive(Debug)]
pub struct RequestBody {
    body: Option<RawBody>,
    is_upgraded: bool,
}

impl From<RawBody> for RequestBody {
    fn from(body: RawBody) -> Self {
        Self {
            body: Some(body),
            is_upgraded: false,
        }
    }
}

impl RequestBody {
    /// Returns 'true' if the instance of raw message body has already taken away.
    pub fn is_gone(&self) -> bool {
        self.body.is_none()
    }

    /// Creates an instance of "Payload" from the raw message body.
    pub fn raw(&mut self) -> Option<RawBody> {
        self.body.take()
    }

    /// Creates an instance of "ReadAll" from the raw message body.
    pub fn read_all(&mut self) -> ReadAll {
        ReadAll {
            state: ReadAllState::Init(self.body.take()),
        }
    }

    /// Returns 'true' if the upgrade function is set.
    pub fn is_upgraded(&self) -> bool {
        self.is_upgraded
    }

    /// Registers the upgrade function to this request.
    pub fn upgrade<F, R>(&mut self, on_upgrade: F) -> Result<(), F>
    where
        F: FnOnce(UpgradedIo) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        if self.is_upgraded {
            return Err(on_upgrade);
        }
        self.is_upgraded = true;

        let body = self.body.take().expect("The body has already gone");
        rt::spawn(
            body.on_upgrade()
                .map_err(|_| ())
                .and_then(move |upgraded| on_upgrade(upgraded).into_future()),
        );

        Ok(())
    }
}

// ==== ReadAll ====

/// A future to receive the entire of incoming message body.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct ReadAll {
    state: ReadAllState,
}

#[derive(Debug)]
enum ReadAllState {
    Init(Option<RawBody>),
    Receiving(RawBody, BytesMut),
    Done,
}

impl ReadAll {
    /// Attempts to receive the entire of message data.
    pub fn poll_ready(&mut self) -> Poll<Bytes, CritError> {
        use self::ReadAllState::*;
        loop {
            match self.state {
                Init(..) => {}
                Receiving(ref mut body, ref mut buf) => {
                    while let Some(chunk) = futures::try_ready!(body.poll_data()) {
                        buf.extend_from_slice(&*chunk);
                    }
                }
                Done => panic!("cannot resolve twice"),
            }

            match mem::replace(&mut self.state, Done) {
                Init(Some(body)) => {
                    self.state = Receiving(body, BytesMut::new());
                    continue;
                }
                Init(None) => return Err(failure::format_err!("").compat().into()),
                Receiving(_body, buf) => {
                    // debug_assert!(body.is_end_stream());
                    return Ok(Async::Ready(buf.freeze()));
                }
                Done => unreachable!(),
            }
        }
    }

    #[doc(hidden)]
    #[deprecated(since = "0.3.3")]
    #[allow(deprecated)]
    pub fn convert_to<T>(self) -> ConvertTo<T>
    where
        T: FromData + 'static,
    {
        ConvertTo {
            read_all: self,
            _marker: PhantomData,
        }
    }
}

impl Future for ReadAll {
    type Item = Bytes;
    type Error = CritError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.poll_ready()
    }
}

// ==== ConvertTo ====

#[doc(hidden)]
#[deprecated(since = "0.3.3")]
#[must_use = "futures do nothing unless polled"]
pub struct ConvertTo<T> {
    read_all: ReadAll,
    _marker: PhantomData<fn() -> T>,
}

#[allow(deprecated)]
impl<T> fmt::Debug for ConvertTo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConvertTo")
            .field("read_all", &self.read_all)
            .finish()
    }
}

#[allow(deprecated)]
impl<T> ConvertTo<T>
where
    T: FromData,
{
    /// Attempts to convert the incoming message data into an value of `T`.
    pub fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<T, Error> {
        let data = futures::try_ready!(self.read_all.poll().map_err(Error::critical));
        T::from_data(data, input)
            .map(Async::Ready)
            .map_err(Into::into)
    }
}

#[allow(deprecated)]
impl<T> Future for ConvertTo<T>
where
    T: FromData,
{
    type Item = T;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let data = futures::try_ready!(self.read_all.poll().map_err(Error::critical));
        with_get_current(|input| T::from_data(data, input))
            .map(Async::Ready)
            .map_err(Into::into)
    }
}

#[doc(hidden)]
#[deprecated(since = "0.3.3")]
pub trait FromData: Sized {
    type Error: Into<Error>;
    fn from_data(data: Bytes, input: &mut Input<'_>) -> Result<Self, Self::Error>;
}

#[allow(deprecated)]
impl FromData for String {
    type Error = Failure;

    fn from_data(data: Bytes, input: &mut Input<'_>) -> Result<Self, Self::Error> {
        if let Some(m) = content_type(input)? {
            if m.type_() != mime::TEXT || m.subtype() != mime::PLAIN {
                return Err(Failure::bad_request(failure::format_err!(
                    "the content type must be text/plain"
                )));
            }
            if m.get_param("charset")
                .map_or(true, |charset| charset != "utf-8")
            {
                return Err(Failure::bad_request(failure::format_err!(
                    "the charset must be utf-8"
                )));
            }
        }

        Self::from_utf8(data.to_vec()).map_err(Failure::bad_request)
    }
}

/// The instance of `FromInput` which parses the message body as an UTF-8 string
/// and converts it into a value by using `serde_plain`.
#[derive(Debug)]
pub struct Plain<T = String>(pub T);

impl<T> Plain<T> {
    #[allow(missing_docs)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl AsRef<str> for Plain<String> {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<T> std::ops::Deref for Plain<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromInput for Plain<T> where T: DeserializeOwned + 'static {}
impl<T> FromInputImpl for Plain<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = Error;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        if let Some(mime) = content_type(input)? {
            if mime.type_() != mime::TEXT || mime.subtype() != mime::PLAIN {
                return Err(crate::error::bad_request(
                    "The content type must be equal to `text/plain`.",
                ).into());
            }
            if let Some(charset) = mime.get_param("charset") {
                if charset != "utf-8" {
                    return Err(crate::error::bad_request(
                        "The charset in content type must be `utf-8`.",
                    ).into());
                }
            }
        }
        Ok(Preflight::Partial(()))
    }

    fn extract(data: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        let s = std::str::from_utf8(&*data).map_err(Failure::bad_request)?;
        serde_plain::from_str(s)
            .map_err(|err| Failure::bad_request(err).into())
            .map(Plain)
    }
}

/// The instance of `FromInput` which deserializes the message body
/// into a JSON value by using `serde_json`.
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    #[allow(missing_docs)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromInput for Json<T> where T: DeserializeOwned + 'static {}
impl<T> FromInputImpl for Json<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = Error;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        let mime = content_type(input)?
            .ok_or_else(|| crate::error::bad_request("missing content-type"))?;
        if *mime != mime::APPLICATION_JSON {
            return Err(
                crate::error::bad_request("The content type must be `application/json`").into(),
            );
        }
        Ok(Preflight::Partial(()))
    }

    fn extract(data: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        serde_json::from_slice(&*data)
            .map_err(|err| Failure::bad_request(err).into())
            .map(Json)
    }
}

/// The instance of `FromInput` which deserializes the message body
/// into a value by using `serde_urlencoded`.
#[derive(Debug)]
pub struct Urlencoded<T>(pub T);

impl<T> Urlencoded<T> {
    #[allow(missing_docs)]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Urlencoded<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromInput for Urlencoded<T> where T: DeserializeOwned + 'static {}
impl<T> FromInputImpl for Urlencoded<T>
where
    T: DeserializeOwned + 'static,
{
    type Error = Error;
    type Ctx = ();

    fn preflight(input: &mut Input<'_>) -> Result<Preflight<Self>, Self::Error> {
        let mime = content_type(input)?
            .ok_or_else(|| crate::error::bad_request("missing content-type"))?;
        if *mime != mime::APPLICATION_WWW_FORM_URLENCODED {
            return Err(crate::error::bad_request(
                "The content type must be `application/x-www-form-urlencoded`",
            ).into());
        }
        Ok(Preflight::Partial(()))
    }

    fn extract(data: &Bytes, _: &mut Input<'_>, _: ()) -> Result<Self, Self::Error> {
        serde_urlencoded::from_bytes(&*data)
            .map_err(|err| Failure::bad_request(err).into())
            .map(Urlencoded)
    }
}
