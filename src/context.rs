//! Components for managing the contextural information throughout the handling.

use cookie::{Cookie, CookieJar};
use failure;
use http::header::HeaderMap;
use http::{header, Request};
use hyperx::header::Header;
use std::cell::{Cell, RefCell};
use std::ops::{Deref, Index};

#[cfg(feature = "session")]
use cookie::{Key, PrivateJar, SignedJar};

use app::AppState;
use error::Error;
use input::{RequestBody, RequestExt};
use router::Route;

scoped_thread_local!(static CONTEXT: Context);

/// The inner parts of `Context`.
#[derive(Debug)]
pub(crate) struct ContextParts {
    pub(crate) request: Request<RequestBody>,
    pub(crate) route: usize,
    pub(crate) params: Vec<(usize, usize)>,
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
    pub(crate) fn new(request: Request<RequestBody>, route: usize, params: Vec<(usize, usize)>) -> Context {
        Context {
            parts: ContextParts {
                request: request,
                route: route,
                params: params,
                cookies: CookieManager::default(),
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
    pub fn with_route<R>(&self, f: impl FnOnce(&Route) -> R) -> R {
        AppState::with(|state| {
            let route = state.router().get_route(self.parts.route).unwrap();
            f(route)
        })
    }

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> Params {
        Params {
            path: self.request().uri().path(),
            params: &self.parts.params[..],
        }
    }

    /// Returns a proxy object for managing the value of Cookie entries.
    ///
    /// This function will perform parsing when called at first, and returns an `Err`
    /// if the value of header field is invalid.
    pub fn cookies(&self) -> Result<Cookies, Error> {
        if !self.parts.cookies.is_init() {
            self.parts
                .cookies
                .init(self.request().headers())
                .map_err(Error::bad_request)?;
        }
        Ok(Cookies {
            jar: &self.parts.cookies.jar,
        })
    }

    pub(crate) fn into_parts(self) -> ContextParts {
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

#[derive(Debug, Default)]
pub(crate) struct CookieManager {
    jar: RefCell<CookieJar>,
    init: Cell<bool>,
}

impl CookieManager {
    pub(crate) fn is_init(&self) -> bool {
        self.init.get()
    }

    pub(crate) fn init(&self, h: &HeaderMap) -> Result<(), failure::Error> {
        let mut jar = self.jar.borrow_mut();
        for raw in h.get_all(header::COOKIE) {
            let raw_s = raw.to_str()?;
            for s in raw_s.split(";").map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)?.into_owned();
                jar.add_original(cookie);
            }
        }
        self.init.set(true);
        Ok(())
    }

    pub(crate) fn append_to(&self, h: &mut HeaderMap) {
        if !self.is_init() {
            return;
        }

        for cookie in self.jar.borrow().delta() {
            h.insert(header::SET_COOKIE, cookie.encoded().to_string().parse().unwrap());
        }
    }
}

/// [unstable]
/// A proxy object for managing Cookie values.
///
/// This object is a thin wrapper of 'CookieJar' defined at 'cookie' crate,
/// and it provides some *basic* APIs for getting the entries of Cookies from an HTTP request
/// or adding/removing Cookie values into an HTTP response.
#[derive(Debug)]
pub struct Cookies<'a> {
    jar: &'a RefCell<CookieJar>,
}

#[allow(missing_docs)]
impl<'a> Cookies<'a> {
    /// Gets a value of Cookie with the provided name from this jar.
    pub fn get(&self, name: &str) -> Option<Cookie<'static>> {
        self.jar.borrow().get(name).map(ToOwned::to_owned)
    }

    /// Adds the provided entry of Cookie into this jar.
    pub fn add(&self, cookie: Cookie<'static>) {
        self.jar.borrow_mut().add(cookie)
    }

    /// Removes the provided entry of Cookie from this jar.
    pub fn remove(&self, cookie: Cookie<'static>) {
        self.jar.borrow_mut().remove(cookie)
    }

    /// Creates a proxy object to manage signed Cookies, and passes the value to a closure and get its result.
    ///
    /// This method is available only if the feature `session` is enabled.
    #[cfg(feature = "session")]
    pub fn with_signed<R>(&self, key: &Key, f: impl FnOnce(SignedJar) -> R) -> R {
        f(self.jar.borrow_mut().signed(key))
    }

    /// Creates a proxy object to manage encrypted Cookies, and passes the value to a closure and get its result.
    ///
    /// This method is available only if the feature `session` is enabled.
    #[cfg(feature = "session")]
    pub fn with_private<R>(&self, key: &Key, f: impl FnOnce(PrivateJar) -> R) -> R {
        f(self.jar.borrow_mut().private(key))
    }

    #[doc(hidden)]
    #[deprecated(since = "0.1.2", note = "use `Cookies::with_private` instead")]
    #[cfg(feature = "session")]
    pub fn get_private(&self, name: &str) -> Option<Cookie<'static>> {
        AppState::with(|state| self.with_private(state.session().secret_key(), |jar| jar.get(name)))
    }

    #[doc(hidden)]
    #[deprecated(since = "0.1.2", note = "use `Cookies::with_private` instead")]
    #[cfg(feature = "session")]
    pub fn add_private(&self, cookie: Cookie<'static>) {
        AppState::with(|state| self.with_private(state.session().secret_key(), |mut jar| jar.add(cookie)))
    }

    #[doc(hidden)]
    #[deprecated(since = "0.1.2", note = "use `Cookies::with_private` instead")]
    #[cfg(feature = "session")]
    pub fn remove_private(&self, cookie: Cookie<'static>) {
        AppState::with(|state| self.with_private(state.session().secret_key(), |mut jar| jar.remove(cookie)))
    }
}
