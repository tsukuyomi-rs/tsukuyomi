//! Components for managing the contextural information throughout the handling.

use http::Request;
use hyperx::header::Header;
use std::ops::{Deref, Index};

use app::AppState;
use error::Error;
use input::{RequestBody, RequestExt};
use router::Route;

#[cfg(feature = "session")]
use session::CookieManager;

#[cfg(feature = "session")]
pub use session::Cookies;

scoped_thread_local!(static CONTEXT: Context);

/// The inner parts of `Context`.
#[derive(Debug)]
pub struct ContextParts {
    pub request: Request<RequestBody>,
    pub(crate) route: Option<(usize, Vec<(usize, usize)>)>,
    #[cfg(feature = "session")]
    pub(crate) cookies: CookieManager,
    _priv: (),
}

/// Contextural values used by handlers during processing an incoming HTTP request.
///
/// The values of this type are created at calling `AppService::call`, and used until the future
/// created at its time is completed.
#[derive(Debug)]
pub struct Context {
    parts: ContextParts,
}

impl Context {
    /// Creates a new instance of `Context` from the provided components.
    pub fn new(request: Request<RequestBody>) -> Context {
        Context {
            parts: ContextParts {
                request: request,
                route: None,
                #[cfg(feature = "session")]
                cookies: Default::default(),
                _priv: (),
            },
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
        &self.parts.request
    }

    /// Parses a header field in the request to a value of `H`.
    pub fn header<H>(&self) -> Result<Option<H>, Error>
    where
        H: Header,
    {
        // TODO: cache the parsed values
        self.request().header()
    }

    /// Runs a closure using the reference to a `Route` matched to the incoming request.
    pub fn with_route<R>(&self, f: impl FnOnce(&Route) -> R) -> Option<R> {
        AppState::with(|state| match self.parts.route {
            Some((i, ..)) => state.router().get_route(i).map(f),
            _ => None,
        })
    }

    pub(crate) fn set_route(&mut self, i: usize, params: Vec<(usize, usize)>) {
        self.parts.route = Some((i, params));
    }

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> Option<Params> {
        match self.parts.route {
            Some((_, ref params)) => Some(Params {
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
        if !self.parts.cookies.is_init() {
            self.parts
                .cookies
                .init(self.request().headers())
                .map_err(Error::internal_server_error)?;
        }
        Ok(self.parts.cookies.cookies())
    }

    /// Consumes itself and convert it into a `ContextParts`.
    pub fn into_parts(self) -> ContextParts {
        self.parts
    }
}

impl Deref for Context {
    type Target = Request<RequestBody>;

    fn deref(&self) -> &Self::Target {
        self.request()
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
