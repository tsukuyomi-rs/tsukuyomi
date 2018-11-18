use cookie::CookieJar;
use http::header::HeaderMap;
use http::Request;

use crate::error::{Critical, Error};
use crate::input::local_map::LocalMap;
use crate::input::Cookies;
use crate::output::Output;

#[derive(Debug)]
pub struct Context<'a> {
    pub(super) request: &'a Request<()>,
    pub(super) locals: &'a mut LocalMap,
    pub(super) response_headers: &'a mut Option<HeaderMap>,
    pub(super) cookies: &'a mut Option<CookieJar>,
}

impl<'a> Context<'a> {
    pub fn request(&self) -> &Request<()> {
        &*self.request
    }

    pub fn locals(&self) -> &LocalMap {
        &*self.locals
    }

    pub fn locals_mut(&mut self) -> &mut LocalMap {
        &mut *self.locals
    }

    pub fn response_headers(&mut self) -> &mut HeaderMap {
        self.response_headers.get_or_insert_with(Default::default)
    }

    pub fn cookies(&mut self) -> Result<Cookies<'_>, Error> {
        Cookies::init(&mut *self.cookies, self.request.headers())
    }
}

pub trait Callback: Send + Sync + 'static {
    #[allow(unused_variables)]
    fn on_init(&self, cx: &mut Context<'_>) -> Result<Option<Output>, Error> {
        Ok(None)
    }

    fn on_error(&self, err: Error, cx: &mut Context<'_>) -> Result<Output, Critical> {
        err.into_response(cx.request)
    }
}

impl Callback for () {}
