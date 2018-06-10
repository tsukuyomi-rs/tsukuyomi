//! Components for managing the contextural information throughout the handling.

use http::Request;
use hyperx::header::Header;
use std::ops::{Deref, Index};
use std::sync::Arc;

use app::AppState;
use error::Error;
use input::{RequestBody, RequestExt};
use router::{Route, RouterState};

#[cfg(feature = "session")]
use session::CookieManager;

#[cfg(feature = "session")]
pub use session::Cookies;

scoped_thread_local!(static CONTEXT: Context);

/// All of contextural information per a request handling, used by the framework.
#[derive(Debug)]
pub struct Context {
    pub(crate) request: Request<RequestBody>,
    route: Option<RouterState>,
    pub(crate) state: Arc<AppState>,
    #[cfg(feature = "session")]
    pub(crate) cookies: CookieManager,
}

impl Context {
    pub(crate) fn new(request: Request<RequestBody>, state: Arc<AppState>) -> Context {
        Context {
            request: request,
            route: None,
            state: state,
            #[cfg(feature = "session")]
            cookies: Default::default(),
        }
    }

    pub(crate) fn set<R>(&self, f: impl FnOnce() -> R) -> R {
        CONTEXT.set(self, f)
    }

    /// Returns 'true' if the reference to a `Context` is set to the scoped TLS.
    ///
    /// If this function returns 'false', the function `Context::with` will panic.
    pub fn is_set() -> bool {
        CONTEXT.is_set()
    }

    /// Executes a closure by using a reference to a `Context` from the scoped TLS and returns its
    /// result.
    ///
    /// # Panics
    /// This function will panic if any reference to `Context` is set to the scoped TLS.
    /// Do not call this function outside the manage of the framework.
    pub fn with<R>(f: impl FnOnce(&Context) -> R) -> R {
        CONTEXT.with(f)
    }

    /// Returns a reference to the value of `Request` contained in this context.
    pub fn request(&self) -> &Request<RequestBody> {
        &self.request
    }

    /// Parses a header field in the request to a value of `H`.
    pub fn header<H>(&self) -> Result<Option<H>, Error>
    where
        H: Header,
    {
        // TODO: cache the parsed values
        self.request.header()
    }

    /// Returns the reference to a `Route` matched to the incoming request.
    pub fn route(&self) -> Option<&Route> {
        match self.route {
            Some(RouterState::Matched(i, ..)) => self.state.router().get_route(i),
            _ => None,
        }
    }

    pub(crate) fn set_route(&mut self, state: RouterState) {
        self.route = Some(state);
    }

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> Option<Params> {
        match self.route {
            Some(RouterState::Matched(_, ref params)) => Some(Params {
                path: self.request().uri().path(),
                params: &params[..],
            }),
            _ => None,
        }
    }

    /// Returns a proxy object for managing the value of Cookie entries.
    ///
    /// This function will perform parsing when called at first, and returns an `Err` if
    /// the value of header field is invalid.
    ///
    /// This function is available only if the feature "session" is enabled.
    #[cfg(feature = "session")]
    pub fn cookies(&self) -> Result<Cookies, Error> {
        if self.cookies.is_init() {
            self.cookies
                .init(self.request.headers())
                .map_err(Error::internal_server_error)?;
        }
        Ok(self.cookies.cookies(self.state.secret_key()))
    }
}

impl Deref for Context {
    type Target = Request<RequestBody>;

    fn deref(&self) -> &Self::Target {
        &self.request
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct Params<'a> {
    path: &'a str,
    params: &'a [(usize, usize)],
}

#[allow(missing_docs)]
impl<'a> Params<'a> {
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    pub fn len(&self) -> usize {
        self.params.len()
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.params.get(i).and_then(|&(s, e)| self.path.get(s..e))
    }

    pub fn iter(&self) -> impl Iterator<Item = &'a str> + 'a {
        let path = self.path;
        self.params.into_iter().map(move |&(s, e)| &path[s..e])
    }
}

impl<'a> Index<usize> for Params<'a> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}
