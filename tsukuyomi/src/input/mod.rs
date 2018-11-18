//! Components for accessing HTTP requests and global/request-local data.

pub mod body;
pub mod local_map;

pub use self::body::RequestBody;

use {
    self::local_map::LocalMap,
    cookie::{Cookie, CookieJar},
    crate::{
        app::{App, AppContext},
        error::Error,
        recognizer::Captures,
        uri::CaptureNames,
    },
    futures::IntoFuture,
    http::{header::HeaderMap, Request},
    mime::Mime,
    std::{cell::Cell, marker::PhantomData, ops::Index, ptr::NonNull, rc::Rc},
};

/// Contextual information used by processes during an incoming HTTP request.
#[derive(Debug)]
pub struct Input<'task> {
    pub(crate) request: &'task Request<()>,
    pub(crate) locals: &'task mut LocalMap,
    pub(crate) response_headers: &'task mut Option<HeaderMap>,
    pub(crate) cookies: &'task mut Option<CookieJar>,
    pub(crate) context: &'task mut AppContext,
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

    /// Creates an instance of "Payload" from the raw message body.
    pub fn take_body(&mut self) -> Option<self::body::RequestBody> {
        self.context.take_body()
    }

    /// Creates an instance of "ReadAll" from the raw message body.
    pub fn read_all(&mut self) -> Option<self::body::ReadAll> {
        self.take_body().map(self::body::ReadAll::new)
    }

    /// Returns 'true' if the upgrade function is set.
    pub fn is_upgraded(&self) -> bool {
        self.context.is_upgraded()
    }

    /// Registers the upgrade function to this request.
    #[inline]
    pub fn upgrade<F, R>(&mut self, on_upgrade: F) -> Result<(), F>
    where
        F: FnOnce(self::body::UpgradedIo) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        self.context.upgrade(on_upgrade)
    }

    /// Returns a reference to the parsed value of `Content-type` stored in the specified `Input`.
    pub fn content_type(&mut self) -> Result<Option<&Mime>, Error> {
        parse_content_type(self.request.headers(), &mut *self.locals)
    }

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> Params<'_> {
        Params::new(
            self.request.uri().path(),
            self.app.uri(self.context.route_id()).capture_names(),
            self.context.captures(),
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
        self.app.get_state(self.context.route_id())
    }

    /// Returns a proxy object for managing the value of Cookie entries.
    ///
    /// This function will perform parsing when called at first, and returns an `Err`
    /// if the value of header field is invalid.
    pub fn cookies(&mut self) -> Result<Cookies<'_>, Error> {
        Cookies::init(&mut *self.cookies, self.request.headers())
    }

    /// Returns a reference to `LocalMap` for managing request-local data.
    #[cfg_attr(tarpaulin, skip)]
    #[inline]
    pub fn locals(&self) -> &LocalMap {
        &*self.locals
    }

    /// Returns a mutable reference to `LocalMap` for managing request-local data.
    #[cfg_attr(tarpaulin, skip)]
    #[inline]
    pub fn locals_mut(&mut self) -> &mut LocalMap {
        &mut *self.locals
    }

    pub fn response_headers(&mut self) -> &mut HeaderMap {
        self.response_headers.get_or_insert_with(Default::default)
    }
}

/// A proxy object for accessing Cookie values.
///
/// Currently this type is a thin wrapper of `&mut cookie::CookieJar`.
#[derive(Debug)]
pub struct Cookies<'a> {
    jar: &'a mut CookieJar,
    _marker: PhantomData<Rc<()>>,
}

impl<'a> Cookies<'a> {
    pub(crate) fn init(jar: &'a mut Option<CookieJar>, h: &HeaderMap) -> Result<Self, Error> {
        if let Some(ref mut jar) = jar {
            return Ok(Cookies {
                jar,
                _marker: PhantomData,
            });
        }

        let jar = jar.get_or_insert_with(CookieJar::new);

        for raw in h.get_all(http::header::COOKIE) {
            let raw_s = raw.to_str().map_err(crate::error::bad_request)?;
            for s in raw_s.split(';').map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)
                    .map_err(crate::error::bad_request)?
                    .into_owned();
                jar.add_original(cookie);
            }
        }

        Ok(Self {
            jar,
            _marker: PhantomData,
        })
    }

    /// Returns a reference to a Cookie value with the specified name.
    #[inline]
    pub fn get(&self, name: &str) -> Option<&Cookie<'static>> {
        self.jar.get(name)
    }

    /// Adds a Cookie entry into jar.
    #[inline]
    pub fn add(&mut self, cookie: Cookie<'static>) {
        self.jar.add(cookie);
    }

    /// Removes a Cookie entry from jar.
    #[inline]
    pub fn remove(&mut self, cookie: Cookie<'static>) {
        self.jar.remove(cookie);
    }

    /// Removes a Cookie entry *completely*.
    #[inline]
    pub fn force_remove(&mut self, cookie: Cookie<'static>) {
        self.jar.force_remove(cookie);
    }
}

#[cfg(feature = "secure")]
mod secure {
    use cookie::{Key, PrivateJar, SignedJar};

    impl<'a> super::Cookies<'a> {
        /// Creates a `SignedJar` with the specified secret key.
        #[inline]
        pub fn signed(&mut self, key: &Key) -> SignedJar<'_> {
            self.jar.signed(key)
        }

        /// Creates a `PrivateJar` with the specified secret key.
        #[inline]
        pub fn private(&mut self, key: &Key) -> PrivateJar<'_> {
            self.jar.private(key)
        }
    }
}

fn parse_content_type<'a>(
    headers: &HeaderMap,
    locals: &'a mut LocalMap,
) -> Result<Option<&'a Mime>, Error> {
    use crate::input::local_map::local_key;
    use crate::input::local_map::Entry;

    local_key! {
        static KEY: Option<Mime>;
    }

    match locals.entry(&KEY) {
        Entry::Occupied(entry) => Ok(entry.into_mut().as_ref()),
        Entry::Vacant(entry) => {
            let mime = match headers.get(http::header::CONTENT_TYPE) {
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

/// A proxy object for accessing extracted parameters.
#[derive(Debug)]
pub struct Params<'input> {
    path: &'input str,
    names: Option<&'input CaptureNames>,
    captures: Option<&'input Captures>,
}

impl<'input> Params<'input> {
    pub(crate) fn new(
        path: &'input str,
        names: Option<&'input CaptureNames>,
        captures: Option<&'input Captures>,
    ) -> Params<'input> {
        debug_assert_eq!(names.is_some(), captures.is_some());
        Params {
            path,
            names,
            captures,
        }
    }

    /// Returns `true` if the extracted paramater exists.
    pub fn is_empty(&self) -> bool {
        self.captures.map_or(true, |caps| {
            caps.params().is_empty() && caps.wildcard().is_none()
        })
    }

    /// Returns the value of `i`-th parameter, if exists.
    pub fn get(&self, i: usize) -> Option<&str> {
        let &(s, e) = self.captures?.params().get(i)?;
        self.path.get(s..e)
    }

    /// Returns the value of wildcard parameter, if exists.
    pub fn get_wildcard(&self) -> Option<&str> {
        let (s, e) = self.captures?.wildcard()?;
        self.path.get(s..e)
    }

    /// Returns the value of parameter whose name is equal to `name`, if exists.
    pub fn name(&self, name: &str) -> Option<&str> {
        match name {
            "*" => self.get_wildcard(),
            name => self.get(self.names?.get_position(name)?),
        }
    }
}

impl<'input> Index<usize> for Params<'input> {
    type Output = str;

    fn index(&self, i: usize) -> &Self::Output {
        self.get(i).expect("Out of range")
    }
}

impl<'input, 'a> Index<&'a str> for Params<'input> {
    type Output = str;

    fn index(&self, name: &'a str) -> &Self::Output {
        self.name(name).expect("Out of range")
    }
}

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

/// Returns `true` if the reference to `Input` is set to the current task.
#[inline(always)]
pub fn is_set_current() -> bool {
    INPUT.with(|input| input.get().is_some())
}

#[cfg_attr(feature = "cargo-clippy", allow(cast_ptr_alignment))]
pub(crate) fn with_set_current<R>(self_: &mut Input<'_>, f: impl FnOnce() -> R) -> R {
    // safety: The value of `self: &mut Input` is always non-null.
    let prev = INPUT.with(|input| {
        let ptr = self_ as *mut Input<'_> as *mut () as *mut Input<'static>;
        input.replace(Some(unsafe { NonNull::new_unchecked(ptr) }))
    });
    let _reset = ResetOnDrop(prev);
    f()
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
/// with_get_current(|input| {
///     some_process()
/// });
///
/// fn some_process() {
///     // Duplicate borrowing of `Input` occurs at this point.
///     with_get_current(|input| { ... })
/// }
/// ```
pub fn with_get_current<R>(f: impl FnOnce(&mut Input<'_>) -> R) -> R {
    let input_ptr = INPUT.with(|input| input.replace(None));
    let _reset = ResetOnDrop(input_ptr);
    let mut input_ptr =
        input_ptr.expect("Any reference to Input are not set at the current task context.");
    // safety: The lifetime of `input_ptr` is always shorter then the borrowing of `Input` in `with_set_current()`
    f(unsafe { input_ptr.as_mut() })
}
