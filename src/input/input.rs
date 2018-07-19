//! Components for managing the contextural information throughout the handling.

use http::Request;
use hyperx::header::Header;
use std::cell::Cell;
use std::ops::{Deref, DerefMut, Index};
use std::ptr::NonNull;

use app::{App, RouteId};
use error::Error;
use input::RequestBody;

use super::cookie::{CookieManager, Cookies};
use super::local_map::LocalMap;

thread_local! {
    static INPUT: Cell<Option<NonNull<Input<'static>>>> = Cell::new(None);
}

#[allow(missing_debug_implementations)]
struct ResetOnDrop(Option<NonNull<Input<'static>>>);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        INPUT.with(|input| {
            input.set(self.0.take());
        })
    }
}

fn with_set_current<R>(self_: &mut Input, f: impl FnOnce() -> R) -> R {
    // safety: The value of `self: &mut Input` is always non-null.
    let prev = INPUT.with(|input| {
        let ptr = self_ as *mut Input as *mut () as *mut Input<'static>;
        input.replace(Some(unsafe { NonNull::new_unchecked(ptr) }))
    });
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
    pub(crate) route: RouteId,
    pub(crate) params: Vec<(usize, usize)>,
    pub(crate) cookies: CookieManager,
    pub(crate) locals: LocalMap,
    _priv: (),
}

impl InputParts {
    pub(crate) fn new(route: RouteId, params: Vec<(usize, usize)>) -> InputParts {
        InputParts {
            route,
            params,
            cookies: CookieManager::new(),
            locals: LocalMap::new(),
            _priv: (),
        }
    }
}

/// Contextual information used by processes during an incoming HTTP request.
#[derive(Debug)]
pub struct Input<'task> {
    pub(crate) request: &'task mut Request<RequestBody>,
    pub(crate) parts: &'task mut InputParts,
    pub(crate) app: &'task App,
}

impl<'task> Input<'task> {
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
    pub fn with_current<R>(f: impl FnOnce(&mut Input) -> R) -> R {
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
        self.request
    }

    /// Returns a mutable reference to the value of `Request` contained in this context.
    pub fn request_mut(&mut self) -> &mut Request<RequestBody> {
        self.request
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

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> Params {
        Params {
            path: self.request.uri().path(),
            params: Some(&self.parts.params[..]),
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
        self.try_get().expect("The value of this value is not set.")
    }

    /// Returns the reference to a value of `T` registered in the global storage, if possible.
    ///
    /// This method will return a `None` if a value of `T` is not registered in the global storage.
    #[inline]
    pub fn try_get<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.app.get(self.parts.route)
    }

    /// Returns a proxy object for managing the value of Cookie entries.
    ///
    /// This function will perform parsing when called at first, and returns an `Err`
    /// if the value of header field is invalid.
    pub fn cookies(&mut self) -> Result<Cookies, Error> {
        let cookies = &mut self.parts.cookies;
        if !cookies.is_init() {
            cookies.init(self.request.headers()).map_err(Error::bad_request)?;
        }
        Ok(cookies.cookies())
    }

    #[allow(missing_docs)]
    pub fn locals(&self) -> &LocalMap {
        &self.parts.locals
    }

    #[allow(missing_docs)]
    pub fn locals_mut(&mut self) -> &mut LocalMap {
        &mut self.parts.locals
    }
}

impl<'task> Deref for Input<'task> {
    type Target = Request<RequestBody>;

    fn deref(&self) -> &Self::Target {
        self.request()
    }
}

impl<'task> DerefMut for Input<'task> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.request_mut()
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct Params<'input> {
    path: &'input str,
    params: Option<&'input [(usize, usize)]>,
}

#[allow(missing_docs)]
impl<'input> Params<'input> {
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

impl<'input> Index<usize> for Params<'input> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}
