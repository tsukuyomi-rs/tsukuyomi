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
///
/// The values of this type are created at calling `AppService::call`, and used until the future
/// created at its time is completed.
#[derive(Debug)]
pub struct Input {
    pub(crate) parts: InputParts,
    pub(crate) state: Arc<AppState>,
}

impl Input {
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
            unsafe {
                input.set(Some(NonNull::new_unchecked(self as *mut Input)));
            }
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
