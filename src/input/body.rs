//! Components for receiving incoming request bodies.

use bytes::{Bytes, BytesMut};
use futures::{Async, Future, IntoFuture, Poll};
use mime;
use std::marker::PhantomData;
use std::{fmt, mem};

use crate::error::{Error, Failure};
use crate::server::rt;
use crate::server::service::http::{Payload as _Payload, RequestBody as RawBody, UpgradedIo};
use crate::server::CritError;

use super::global::with_get_current;
use super::header::content_type;
use super::Input;

// ==== RequestBody ====

/// A type representing a message body in the incoming HTTP request.
#[derive(Debug)]
pub struct RequestBody {
    body: Option<RawBody>,
    is_upgraded: bool,
}

impl From<RawBody> for RequestBody {
    fn from(body: RawBody) -> RequestBody {
        RequestBody {
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

    /// Creates a future from `self` that will convert the received data into a value of `T`.
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

/// A future to receive the entire of message body and then convert the data into a value of `T`.
#[must_use = "futures do nothing unless polled"]
pub struct ConvertTo<T> {
    read_all: ReadAll,
    _marker: PhantomData<fn() -> T>,
}

impl<T> fmt::Debug for ConvertTo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConvertTo")
            .field("read_all", &self.read_all)
            .finish()
    }
}

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

/// A trait representing the conversion to certain type.
pub trait FromData: Sized {
    /// The error type which will be returned from `from_data`.
    type Error: Into<Error>;

    /// Perform conversion from a received buffer of bytes into a value of `Self`.
    fn from_data(data: Bytes, input: &mut Input<'_>) -> Result<Self, Self::Error>;
}

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

        String::from_utf8(data.to_vec()).map_err(Failure::bad_request)
    }
}
