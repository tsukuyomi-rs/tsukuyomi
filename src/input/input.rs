//! Components for managing the contextural information throughout the handling.

use http::Request;
use hyperx::header::Header;
use std::cell::Cell;
use std::ops::{Deref, DerefMut, Index};
use std::ptr::NonNull;
use std::sync::Arc;

use app::AppState;
use error::Error;
use input::RequestBody;
use router::Endpoint;

use super::cookie::{CookieManager, Cookies};

thread_local! {
    static INPUT: Cell<Option<NonNull<Input>>> = Cell::new(None);
}

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
                cookies: CookieManager::new(),
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
        match self.headers().get(H::header_name()) {
            Some(h) => H::parse_header(&h.as_bytes().into())
                .map_err(Error::bad_request)
                .map(Some),
            None => Ok(None),
        }
    }

    /// Returns the reference to a `Endpoint` matched to the incoming request.
    pub fn endpoint(&self) -> &Endpoint {
        self.global()
            .router()
            .get(self.parts.route)
            .expect("The wrong route ID")
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
    pub fn cookies(&mut self) -> Result<Cookies, Error> {
        let cookies = &mut self.parts.cookies;
        if !cookies.is_init() {
            cookies.init(self.parts.request.headers()).map_err(Error::bad_request)?;
        }
        Ok(cookies.cookies())
    }

    /// Returns the reference to the global state.
    pub fn global(&self) -> &AppState {
        &*self.parts.global
    }

    pub(crate) fn into_parts(self) -> InputParts {
        self.parts
    }
}

#[allow(missing_debug_implementations)]
struct ResetOnDrop(Option<NonNull<Input>>);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        INPUT.with(|input| {
            input.set(self.0.take());
        })
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
    pub(crate) fn with_set<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let prev = INPUT.with(|input| {
            let prev = input.get();
            // safety: &mut self is non-null.
            input.set(Some(unsafe { NonNull::new_unchecked(self as *mut Input) }));
            prev
        });
        let _reset = ResetOnDrop(prev);
        let res = f();
        res
    }

    /// Returns `true` if the reference to `Input` is set to the current task.
    ///
    /// * Outside of `Future` managed by the framework.
    /// * A mutable borrow has already been acquired in the higher scope.
    pub fn is_set() -> bool {
        INPUT.with(|input| input.get().is_some())
    }

    /// Retrieves a mutable borrow of `Input` from scoped TLS and executes the provided closure
    /// with its reference.
    ///
    /// # Panics
    /// This function will cause a panic if any reference to `Input` is not set at the current task.
    #[inline]
    pub fn with_get<R>(f: impl FnOnce(&mut Input) -> R) -> R {
        Input::try_with_get(f).expect("failed to acquire a &mut Input from the current task")
    }

    /// Tries to acquire a mutable borrow of `Input` from scoped TLS and executes the provided closure
    /// with its reference if it succeeds.
    ///
    /// This function will return a `None` if any reference to `Input` is not set at the current task.
    #[inline]
    pub fn try_with_get<R>(f: impl FnOnce(&mut Input) -> R) -> Option<R> {
        let input_ptr = INPUT.with(|input| {
            let input_ptr = input.get();
            input.set(None);
            input_ptr
        });
        let _reset = ResetOnDrop(input_ptr);

        match input_ptr {
            Some(mut input_ptr) => Some(unsafe { f(input_ptr.as_mut()) }),
            None => None,
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
