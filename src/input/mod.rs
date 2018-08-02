//! Components for parsing incoming HTTP requests and accessing the global or request-local data.

pub mod body;
#[macro_use]
pub mod local_map;
pub mod upgrade;

mod cookie;
mod global;
mod params;

// re-exports
pub use self::body::RequestBody;
pub(crate) use self::global::with_set_current;
pub use self::global::{is_set_current, with_get_current};
pub use self::params::Params;

#[allow(missing_docs)]
pub mod header {
    use http::header;
    use mime::Mime;

    use error::Error;

    use super::local_map::Entry;
    use super::Input;

    /// Returns a reference to the parsed value of `Content-type` stored in the specified `Input`.
    pub fn content_type<'a>(input: &'a mut Input) -> Result<Option<&'a Mime>, Error> {
        local_key!(static CONTENT_TYPE: Option<Mime>);

        match input.parts.locals.entry(&CONTENT_TYPE) {
            Entry::Occupied(entry) => Ok(entry.into_mut().as_ref()),
            Entry::Vacant(entry) => {
                let mime = match input.request.headers().get(header::CONTENT_TYPE) {
                    Some(h) => h.to_str()
                        .map_err(Error::bad_request)?
                        .parse()
                        .map(Some)
                        .map_err(Error::bad_request)?,
                    None => None,
                };
                Ok(entry.insert(mime).as_ref())
            }
        }
    }
}

// ====

use cookie::CookieJar;
use http::Request;
use std::ops::{Deref, DerefMut};

use app::{App, RouteId};
use error::Error;
use recognizer::Captures;

use self::cookie::CookieManager;
use self::local_map::LocalMap;

/// The inner parts of `Input`.
#[derive(Debug)]
pub(crate) struct InputParts {
    pub(crate) route: RouteId,
    pub(crate) captures: Captures,
    pub(crate) cookies: CookieManager,
    pub(crate) locals: LocalMap,
    _priv: (),
}

impl InputParts {
    pub(crate) fn new(route: RouteId, captures: Captures) -> InputParts {
        InputParts {
            route,
            captures,
            cookies: CookieManager::new(),
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
    /// Returns a shared reference to the value of `Request` contained in this context.
    pub fn request(&self) -> &Request<RequestBody> {
        self.request
    }

    /// Returns a mutable reference to the value of `Request` contained in this context.
    pub fn request_mut(&mut self) -> &mut Request<RequestBody> {
        self.request
    }

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> Params {
        Params {
            path: self.request.uri().path(),
            captures: &self.parts.captures,
        }
    }

    /// Returns the reference to a value of `T` registered in the global storage, if possible.
    ///
    /// This method will return a `None` if a value of `T` is not registered in the global storage.
    #[inline]
    pub fn get<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.app.get(self.parts.route)
    }

    /// Returns a proxy object for managing the value of Cookie entries.
    ///
    /// This function will perform parsing when called at first, and returns an `Err`
    /// if the value of header field is invalid.
    pub fn cookies(&mut self) -> Result<&mut CookieJar, Error> {
        let cookies = &mut self.parts.cookies;
        if !cookies.is_init() {
            cookies.init(self.request.headers()).map_err(Error::bad_request)?;
        }
        Ok(&mut cookies.jar)
    }

    /// Returns a reference to `LocalMap` for managing request-local data.
    pub fn locals(&self) -> &LocalMap {
        &self.parts.locals
    }

    /// Returns a mutable reference to `LocalMap` for managing request-local data.
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
