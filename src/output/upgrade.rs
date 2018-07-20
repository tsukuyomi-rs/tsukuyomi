//! [unstable]
//! Components for HTTP/1.1 upgrade mechanism.

use futures::{Future, IntoFuture};
use http::header::{HeaderName, HeaderValue};
use http::{header, response, HttpTryFrom, Request, Response, StatusCode, Version};
use hyper::upgrade::Upgraded;
use std::{fmt, mem};

use error::Error;
use input::Input;
use output::{Output, Responder, ResponseBody};

/// [unstable]
/// A "Responder" for constructing an upgrade response.
#[derive(Debug)]
pub struct Upgrade {
    response: response::Builder,
    handler: BoxedUpgradeHandler,
}

impl Upgrade {
    /// Creates a builder object for constructing an instance of this type.
    pub fn builder(name: &str) -> UpgradeBuilder {
        let mut response = Response::builder();
        response
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, name);
        UpgradeBuilder { response }
    }
}

impl Responder for Upgrade {
    fn respond_to(mut self, input: &mut Input) -> Result<Output, Error> {
        if input.version() != Version::HTTP_11 {
            // FIXME: choose appropriate status code
            return Err(Error::bad_request(format_err!(
                "Protocol upgrade is available only on HTTP/1.1"
            )));
        }

        let response = self.response
            .body(ResponseBody::empty())
            .map_err(Error::internal_server_error)?;

        Ok(Output {
            response,
            upgrade: Some(self.handler),
        })
    }
}

/// A builder for constructing an "Upgrade".
#[derive(Debug)]
pub struct UpgradeBuilder {
    response: response::Builder,
}

impl UpgradeBuilder {
    /// Append an additional header value to the response.
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        HeaderName: HttpTryFrom<K>,
        HeaderValue: HttpTryFrom<V>,
    {
        self.response.header(key, value);
        self
    }

    /// Perform construction of an "Upgrade".
    pub fn finish<H>(&mut self, handler: H) -> Upgrade
    where
        H: UpgradeHandler + Send + 'static,
        H::Future: Send + 'static,
    {
        Upgrade {
            response: mem::replace(&mut self.response, Response::builder()),
            handler: handler.into(),
        }
    }
}

// ====

/// All of contextural information used when the protocol upgrade is performed.
#[derive(Debug)]
pub struct UpgradeContext {
    /// The underlying IO object used in the handshake.
    pub io: Upgraded,

    /// The value of "Request" received at the last request.
    pub request: Request<()>,

    pub(crate) _priv: (),
}

/// A trait representing performing a protocol upgrade.
pub trait UpgradeHandler {
    /// A "Future" which will be returned from "upgrade".
    type Future: Future<Item = (), Error = ()>;

    /// Constructs a "Future" which operates the upgraded connection.
    fn upgrade(self, cx: UpgradeContext) -> Self::Future;
}

impl<F, R> UpgradeHandler for F
where
    F: FnOnce(UpgradeContext) -> R,
    R: IntoFuture<Item = (), Error = ()>,
{
    type Future = R::Future;

    fn upgrade(self, cx: UpgradeContext) -> Self::Future {
        (self)(cx).into_future()
    }
}

pub(crate) struct BoxedUpgradeHandler {
    inner: Box<dyn FnMut(UpgradeContext) -> Box<dyn Future<Item = (), Error = ()> + Send> + Send + 'static>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for BoxedUpgradeHandler {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BoxedUpgradeHandler").finish()
    }
}

impl<H> From<H> for BoxedUpgradeHandler
where
    H: UpgradeHandler + Send + 'static,
    H::Future: Send + 'static,
{
    fn from(handler: H) -> Self {
        let mut handler = Some(handler);
        BoxedUpgradeHandler {
            inner: Box::new(move |cx| {
                let handler = handler.take().expect("cannot upgrade twice");
                Box::new(handler.upgrade(cx))
            }),
        }
    }
}

impl BoxedUpgradeHandler {
    pub(crate) fn upgrade(mut self, cx: UpgradeContext) -> Box<dyn Future<Item = (), Error = ()> + Send + 'static> {
        (self.inner)(cx)
    }
}
