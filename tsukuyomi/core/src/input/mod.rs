//! Components for parsing incoming HTTP requests and accessing the global or request-local data.

#[macro_use]
pub mod local_map;
pub mod body;
mod cookie;
mod global;
pub mod param;

#[allow(missing_docs)]
pub mod header {
    use http::header;
    use mime::Mime;

    use crate::error::Error;

    use super::local_map::Entry;
    use super::Input;

    /// Returns a reference to the parsed value of `Content-type` stored in the specified `Input`.
    pub fn content_type<'a>(input: &'a mut Input<'_>) -> Result<Option<&'a Mime>, Error> {
        local_key!(static CONTENT_TYPE: Option<Mime>);

        match input.parts.locals.entry(&CONTENT_TYPE) {
            Entry::Occupied(entry) => Ok(entry.into_mut().as_ref()),
            Entry::Vacant(entry) => {
                let mime = match input.request.headers().get(header::CONTENT_TYPE) {
                    Some(h) => h
                        .to_str()
                        .map_err(crate::error::bad_request)?
                        .parse()
                        .map(Some)
                        .map_err(crate::error::bad_request)?,
                    None => None,
                };
                Ok(entry.insert(mime).as_ref())
            }
        }
    }
}

// re-exports
pub use self::body::RequestBody;
pub use self::cookie::Cookies;
pub(crate) use self::global::with_set_current;
pub use self::global::{is_set_current, with_get_current};

// ====

use http::Request;
use std::cell::UnsafeCell;
use std::fmt;
use std::marker::PhantomData;

use crate::app::route::RouteId;
use crate::app::App;
use crate::error::Error;
use crate::recognizer::captures::Captures;

use self::cookie::CookieManager;
use self::local_map::LocalMap;

/// The inner parts of `Input`.
#[derive(Debug)]
pub(crate) struct InputParts {
    pub(crate) route: RouteId,
    pub(crate) captures: Option<Captures>,
    pub(crate) cookies: CookieManager,
    pub(crate) locals: LocalMap,
    _priv: (),
}

impl InputParts {
    pub(crate) fn new(route: RouteId, captures: Option<Captures>) -> Self {
        Self {
            route,
            captures,
            cookies: CookieManager::default(),
            locals: LocalMap::default(),
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
    /// Returns a reference to the HTTP method of the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn method(&self) -> &http::Method {
        self.request.method()
    }

    /// Returns a reference to the URI of the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn uri(&self) -> &http::Uri {
        self.request.uri()
    }

    /// Returns a reference to the HTTP version of the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn version(&self) -> http::Version {
        self.request.version()
    }

    /// Returns a reference to the header map in the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn headers(&self) -> &http::HeaderMap {
        self.request.headers()
    }

    /// Returns a reference to the extensions map in the request.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn extensions(&self) -> &http::Extensions {
        self.request.extensions()
    }

    /// Returns a reference to the instance of `RequestBody`.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn body(&self) -> &RequestBody {
        self.request.body()
    }

    /// Returns a mutable reference to the instance of `RequestBody`.
    #[inline]
    #[cfg_attr(tarpaulin, skip)]
    pub fn body_mut(&mut self) -> &mut RequestBody {
        self.request.body_mut()
    }

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> self::param::Params<'_> {
        self::param::Params::new(
            self.request.uri().path(),
            self.app.uri(self.parts.route).capture_names(),
            self.parts.captures.as_ref(),
        )
    }

    /// Returns the reference to a value of `T` registered in the global storage, if possible.
    ///
    /// This method will return a `None` if a value of `T` is not registered in the global storage.
    #[inline]
    pub fn state<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.app.get_state(self.parts.route)
    }

    /// Returns a proxy object for managing the value of Cookie entries.
    ///
    /// This function will perform parsing when called at first, and returns an `Err`
    /// if the value of header field is invalid.
    pub fn cookies(&mut self) -> Result<self::cookie::Cookies<'_>, Error> {
        self.parts.cookies.init(self.request.headers())
    }

    /// Returns a reference to `LocalMap` for managing request-local data.
    #[cfg_attr(tarpaulin, skip)]
    pub fn locals(&self) -> &LocalMap {
        &self.parts.locals
    }

    /// Returns a mutable reference to `LocalMap` for managing request-local data.
    #[cfg_attr(tarpaulin, skip)]
    pub fn locals_mut(&mut self) -> &mut LocalMap {
        &mut self.parts.locals
    }
}

/// A proxy object for accessing the value in the protocol extensions.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
pub struct Extension<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
}

impl<T> fmt::Debug for Extension<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extension").finish()
    }
}

impl<T> Extension<T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    #[allow(missing_docs)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let state = input.extensions().get::<T>().expect("should be exist");
            f(state)
        })
    }
}

/// A proxy object for accessing the global state.
#[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
pub struct State<T> {
    _marker: PhantomData<(fn() -> T, UnsafeCell<()>)>,
}

impl<T> fmt::Debug for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State").finish()
    }
}

impl<T> State<T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    #[allow(missing_docs)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        crate::input::with_get_current(|input| {
            let state = input.state::<T>().expect("should be exist");
            f(state)
        })
    }
}
