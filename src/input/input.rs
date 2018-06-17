//! Components for managing the contextural information throughout the handling.

use cookie::{Cookie, CookieJar};
use failure;
use http::header::HeaderMap;
use http::{header, Request};
use hyperx::header::Header;
use std::cell::{Cell, RefCell};
use std::ops::{Deref, DerefMut, Index};
use std::sync::Arc;

#[cfg(feature = "session")]
use cookie::{Key, PrivateJar, SignedJar};

use app::AppState;
use error::Error;
use input::{RequestBody, RequestExt};
use router::Route;

scoped_thread_local!(static INPUT: RefCell<Input>);

/// The inner parts of `Input`.
#[derive(Debug)]
pub(crate) struct InputParts {
    pub(crate) request: Request<RequestBody>,
    pub(crate) route: usize,
    pub(crate) params: Vec<(usize, usize)>,
    pub(crate) cookies: CookieManager,
    pub(crate) global: Arc<AppState>,
    _priv: (),
}

/// All of contextural values used by handlers during processing an incoming HTTP request.
///
/// The values of this type are created at calling `AppService::call`, and used until the future
/// created at its time is completed.
#[derive(Debug)]
pub struct Input {
    parts: InputParts,
}

impl Input {
    pub(crate) fn new(
        request: Request<RequestBody>,
        route: usize,
        params: Vec<(usize, usize)>,
        global: Arc<AppState>,
    ) -> Input {
        Input {
            parts: InputParts {
                request: request,
                route: route,
                params: params,
                cookies: CookieManager::default(),
                global: global,
                _priv: (),
            },
        }
    }

    /// Returns a shared reference to the value of `Request` contained in this context.
    pub fn request(&self) -> &Request<RequestBody> {
        &self.parts.request
    }

    /// Returns a mutable reference to the value of `Request` contained in this context.
    pub fn request_mut(&mut self) -> &mut Request<RequestBody> {
        &mut self.parts.request
    }

    /// Parses a header field in the request to a value of `H`.
    pub fn header<H>(&self) -> Result<Option<H>, Error>
    where
        H: Header,
    {
        // TODO: cache the parsed values
        self.request().header()
    }

    /// Returns the reference to a `Route` matched to the incoming request.
    pub fn route(&self) -> &Route {
        self.global()
            .router()
            .get_route(self.parts.route)
            .expect("The wrong route ID")
    }

    pub(crate) fn route_id(&self) -> usize {
        self.parts.route
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

    /// Returns the reference to the global state.
    pub fn global(&self) -> &AppState {
        &*self.parts.global
    }

    pub(crate) fn into_parts(self) -> InputParts {
        self.parts
    }
}

/// A set of functions for accessing the reference to `Input` located at the task local storage.
///
/// These functions only work in `Future`s called directly in `AppServiceFuture`.
/// Do not use these in the following situations:
///
/// * The outside of this framework.
/// * Inside the futures spawned by some `Executor`s (such situation will cause by using
///   `tokio::spawn()`).
///
/// The provided functions forms some dynamic scope for the borrowing the instance of `Input` on
/// the scoped TLS.
/// If these scopes violate the borrowing rule in Rust, a runtime error will reported or cause a
/// panic.
impl Input {
    pub(crate) fn set<R>(self_: &RefCell<Self>, f: impl FnOnce() -> R) -> R {
        INPUT.set(self_, f)
    }

    /// Returns 'true' if the reference to a `Input` is set to the scoped TLS.
    pub fn is_set() -> bool {
        INPUT.is_set()
    }

    /// Retrieves a shared borrow of `Input` from scoped TLS and executes the provided closure
    /// with its reference.
    ///
    /// # Panics
    /// This function will cause a panic if any reference to `Input` is set at the current task or
    /// a mutable borrow has already been exist.
    #[inline]
    pub fn with<R>(f: impl FnOnce(&Input) -> R) -> R {
        Input::try_with(f).expect("in Input::with")
    }

    /// Retrieves a mutable borrow of `Input` from scoped TLS and executes the provided closure
    /// with its reference.
    ///
    /// # Panics
    /// This function will cause a panic if any reference to `Input` is set at the current task or
    /// a mutable or shared borrows have already been exist.
    #[inline]
    pub fn with_mut<R>(f: impl FnOnce(&mut Input) -> R) -> R {
        Input::try_with_mut(f).expect("in Input::with_mut")
    }

    /// Tries to acquire a shared borrow of `Input` from scoped TLS and executes the provided closure
    /// with its reference if it succeeds.
    ///
    /// This function will report an error if any reference to `Input` is set at the current task or
    /// a mutable borrow has already been exist.
    #[inline]
    pub fn try_with<R>(f: impl FnOnce(&Input) -> R) -> Result<R, Error> {
        Input::try_with_inner(|input| {
            let input = input.try_borrow().map_err(Error::internal_server_error)?;
            Ok(f(&*input))
        })
    }

    /// Tries to acquire a mutable borrow of `Input` from scoped TLS and executes the provided closure
    /// with its reference if it succeeds.
    ///
    /// This function will report an error if any reference to `Input` is set at the current task or
    /// a mutable or shared borrows have already been exist.
    #[inline]
    pub fn try_with_mut<R>(f: impl FnOnce(&mut Input) -> R) -> Result<R, Error> {
        Input::try_with_inner(|input| {
            let mut input = input.try_borrow_mut().map_err(Error::internal_server_error)?;
            Ok(f(&mut *input))
        })
    }

    fn try_with_inner<R>(f: impl FnOnce(&RefCell<Input>) -> Result<R, Error>) -> Result<R, Error> {
        if INPUT.is_set() {
            INPUT.with(f)
        } else {
            Err(Error::internal_server_error(format_err!(
                "The reference to Input is not set at the current task."
            )))
        }
    }
}

impl Deref for Input {
    type Target = Request<RequestBody>;

    fn deref(&self) -> &Self::Target {
        self.request()
    }
}

impl DerefMut for Input {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.request_mut()
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
}
