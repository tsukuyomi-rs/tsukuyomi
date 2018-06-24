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

#[allow(missing_debug_implementations)]
struct ResetOnDrop(Option<NonNull<Input>>);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        INPUT.with(|input| {
            input.set(self.0.take());
        })
    }
}

fn with_set_current<R>(self_: &mut Input, f: impl FnOnce() -> R) -> R {
    // safety: The value of `self: &mut Input` is always non-null.
    let prev = INPUT.with(|input| input.replace(Some(unsafe { NonNull::new_unchecked(self_ as *mut Input) })));
    let _reset = ResetOnDrop(prev);
    f()
}

fn with_get_current<R>(f: impl FnOnce(&mut Input) -> R) -> R {
    let input_ptr = INPUT.with(|input| input.replace(None));
    let _reset = ResetOnDrop(input_ptr);
    let mut input_ptr = input_ptr.expect("Any reference to Input are not set at the current task context.");
    // safety: The lifetime of `input_ptr` is always shorter then the borrowing of `Input` in `with_set_current()`
    f(unsafe { input_ptr.as_mut() })
}

/// The inner parts of `Input`.
#[derive(Debug)]
pub(crate) struct InputParts {
    pub(crate) request: Request<RequestBody>,
    pub(crate) route: Option<(usize, Vec<(usize, usize)>)>,
    pub(crate) cookies: CookieManager,
    _priv: (),
}

impl InputParts {
    pub(crate) fn new(request: Request<RequestBody>) -> InputParts {
        InputParts {
            request: request.map(Into::into),
            route: None,
            cookies: CookieManager::new(),
            _priv: (),
        }
    }
}

/// Contextual information used by processes during an incoming HTTP request.
#[derive(Debug)]
pub struct Input {
    pub(crate) parts: InputParts,
    pub(crate) state: Arc<AppState>,
}

impl Input {
    pub(crate) fn with_set_current<R>(&mut self, f: impl FnOnce() -> R) -> R {
        with_set_current(self, f)
    }

    /// Acquires a mutable borrow of `Input` from the current task context and executes the provided
    /// closure with its reference.
    ///
    /// # Panics
    ///
    /// This function only work in the management of the framework and causes a panic
    /// if any references to `Input` is not set at the current task.
    /// Do not use this function outside of futures returned by the handler functions.
    /// Such situations often occurs by spawning tasks by the external `Executor`
    /// (typically calling `tokio::spawn()`).
    ///
    /// In additional, this function forms a (dynamic) scope to prevent the references to `Input`
    /// violate the borrowing rule in Rust.
    /// Duplicate borrowings such as the following code are reported as a runtime error.
    ///
    /// ```ignore
    /// Input::with_current(|input| {
    ///     some_process()
    /// });
    ///
    /// fn some_process() {
    ///     // Duplicate borrowing of `Input` occurs at this point.
    ///     Input::with_current(|input| { ... })
    /// }
    /// ```
    #[inline]
    pub fn with_current<R>(f: impl FnOnce(&mut Self) -> R) -> R {
        with_get_current(f)
    }

    /// Returns `true` if the reference to `Input` is set to the current task.
    ///
    /// * Outside of `Future` managed by the framework.
    /// * A mutable borrow has already been acquired.
    pub fn is_set() -> bool {
        INPUT.with(|input| input.get().is_some())
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
    pub fn endpoint(&self) -> Option<&Endpoint> {
        match self.parts.route {
            Some((i, _)) => self.state.router().get(i),
            None => None,
        }
    }

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> Params {
        match self.parts.route {
            Some((_, ref params)) => Params {
                path: self.request().uri().path(),
                params: Some(&params[..]),
            },
            None => Params {
                path: self.request().uri().path(),
                params: None,
            },
        }
    }

    /// Returns the reference to a value of `T` registered in the global storage.
    ///
    /// # Panics
    /// This method will cause a panic if a value of `T` is not registered in the global storage.
    #[inline]
    pub fn get<T>(&self) -> &T
    where
        T: Send + Sync + 'static,
    {
        self.state.get()
    }

    /// Returns the reference to a value of `T` registered in the global storage, if possible.
    ///
    /// This method will return a `None` if a value of `T` is not registered in the global storage.
    #[inline]
    pub fn try_get<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.state.try_get()
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
    params: Option<&'a [(usize, usize)]>,
}

#[allow(missing_docs)]
impl<'a> Params<'a> {
    pub fn is_empty(&self) -> bool {
        self.params.as_ref().map_or(true, |p| p.is_empty())
    }

    pub fn len(&self) -> usize {
        self.params.as_ref().map_or(0, |p| p.len())
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.params
            .as_ref()
            .and_then(|p| p.get(i).and_then(|&(s, e)| self.path.get(s..e)))
    }
}

impl<'a> Index<usize> for Params<'a> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}
