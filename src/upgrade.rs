use bytes::Bytes;
use futures::{Future, IntoFuture};
use http::header::{HeaderName, HeaderValue};
use http::{header, response, HttpTryFrom, Response, StatusCode};
use std::{fmt, mem};

use context::Context;
use error::Error;
use output::{Output, Responder, ResponseBody};
use transport::Io;

// TODO: validate

pub struct UpgradeContext {
    handler: UpgradeFn,
    response: response::Builder,
}

impl UpgradeContext {
    pub fn builder(name: &str) -> UpgradeBuilder {
        let mut response = Response::builder();
        response
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, name);
        UpgradeBuilder { response: response }
    }
}

impl Responder for UpgradeContext {
    fn respond_to(mut self, _cx: &Context) -> Result<Output, Error> {
        let response = self.response.body(ResponseBody::empty().into_hyp())?;
        Ok(Output {
            response: response,
            upgrade: Some(self.handler),
        })
    }
}

pub struct UpgradeBuilder {
    response: response::Builder,
}

impl UpgradeBuilder {
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        HeaderName: HttpTryFrom<K>,
        HeaderValue: HttpTryFrom<V>,
    {
        self.response.header(key, value);
        self
    }

    pub fn finish<H>(&mut self, handler: H) -> UpgradeContext
    where
        H: UpgradeHandler + Send + 'static,
        H::Future: Send + 'static,
    {
        UpgradeContext {
            handler: handler.into(),
            response: mem::replace(&mut self.response, Response::builder()),
        }
    }
}

// ====

pub trait UpgradeHandler {
    type Future: Future<Item = (), Error = ()>;

    fn upgrade(self, io: Io, read_buf: Bytes, cx: &Context) -> Self::Future;
}

impl<F, R> UpgradeHandler for F
where
    F: FnOnce(Io, Bytes, &Context) -> R,
    R: IntoFuture<Item = (), Error = ()>,
{
    type Future = R::Future;

    fn upgrade(self, io: Io, read_buf: Bytes, cx: &Context) -> Self::Future {
        (self)(io, read_buf, cx).into_future()
    }
}

pub(crate) struct UpgradeFn {
    inner: Box<
        FnMut(Io, Bytes, &Context) -> Box<Future<Item = (), Error = ()> + Send> + Send + 'static,
    >,
}

impl fmt::Debug for UpgradeFn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("UpgradeFn").finish()
    }
}

impl<H> From<H> for UpgradeFn
where
    H: UpgradeHandler + Send + 'static,
    H::Future: Send + 'static,
{
    fn from(handler: H) -> Self {
        let mut handler = Some(handler);
        UpgradeFn {
            inner: Box::new(move |io, read_buf, cx| {
                let handler = handler.take().expect("cannot upgrade twice");
                Box::new(handler.upgrade(io, read_buf, cx))
            }),
        }
    }
}

impl UpgradeFn {
    pub fn upgrade(
        &mut self,
        io: Io,
        read_buf: Bytes,
        cx: &Context,
    ) -> Box<Future<Item = (), Error = ()> + Send + 'static> {
        (self.inner)(io, read_buf, cx)
    }
}
