use {
    super::{App, RouteId},
    cookie::{Cookie, CookieJar},
    crate::{
        async_result::AsyncResult,
        error::{Critical, Error, HttpError},
        input::{local_map::LocalMap, RequestBody},
        output::{Output, ResponseBody},
        recognizer::Captures,
        uri::CaptureNames,
    },
    futures::{Async, Future, IntoFuture, Poll},
    http::{
        header,
        header::{HeaderMap, HeaderValue},
        Method, Request, Response, StatusCode,
    },
    hyper::body::Payload,
    mime::Mime,
    std::{cell::UnsafeCell, marker::PhantomData, ops::Index, rc::Rc},
    tower_service::{NewService, Service},
};

macro_rules! ready {
    ($e:expr) => {
        match $e {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(x)) => Ok(x),
            Err(e) => Err(e),
        }
    };
}

pub fn app() -> super::builder::Builder<(), ()> {
    super::builder::Builder::default()
}

/// An instance of `HttpError` which will be thrown from the route recognizer.
///
/// The value of this type cannot be modified by the `Modifier`s since they will be
/// thrown before the scope will be determined.
#[derive(Debug, failure::Fail)]
pub enum RecognizeError {
    /// The request path is not matched to any routes.
    #[fail(display = "Not Found")]
    NotFound,

    /// The request path is matched but the method is invalid.
    #[fail(display = "Method Not Allowed")]
    MethodNotAllowed,
}

impl HttpError for RecognizeError {
    fn status_code(&self) -> StatusCode {
        match self {
            RecognizeError::NotFound => StatusCode::NOT_FOUND,
            RecognizeError::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
        }
    }
}

#[derive(Debug)]
pub enum Recognize<'a> {
    Matched(usize, Option<Captures>),
    FallbackHead(usize, Option<Captures>),
    FallbackOptions(&'a HeaderValue),
}

impl App {
    #[doc(hidden)]
    pub fn recognize(&self, path: &str, method: &Method) -> Result<Recognize<'_>, RecognizeError> {
        let (i, params) = self
            .data
            .recognizer
            .recognize(path)
            .ok_or_else(|| RecognizeError::NotFound)?;
        let endpoint = &self.data.endpoints[i];

        match endpoint.route_ids.get(method) {
            Some(&i) => Ok(Recognize::Matched(i, params)),
            None if self.data.config.fallback_head && *method == Method::HEAD => {
                match endpoint.route_ids.get(&Method::GET) {
                    Some(&i) => Ok(Recognize::FallbackHead(i, params)),
                    None => Err(RecognizeError::MethodNotAllowed),
                }
            }
            None if self.data.config.fallback_options && *method == Method::OPTIONS => {
                Ok(Recognize::FallbackOptions(&endpoint.allowed_methods))
            }
            None => Err(RecognizeError::MethodNotAllowed),
        }
    }
}

impl NewService for App {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = Critical;
    type Service = Self;
    type InitError = Critical;
    type Future = futures::future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        futures::future::ok(self.clone())
    }
}

impl Service for App {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = Critical;
    type Future = AppFuture;

    #[inline]
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    #[inline]
    fn call(&mut self, request: Self::Request) -> Self::Future {
        let (parts, body) = request.into_parts();
        AppFuture {
            state: AppFutureState::Init,
            app: self.clone(),
            request: Request::from_parts(parts, ()),
            context: AppContext {
                body: Some(body),
                is_upgraded: false,
                locals: LocalMap::default(),
                response_headers: None,
                cookies: None,
                route: None,
            },
        }
    }
}

/// A future for managing an incoming HTTP request, created by `AppService`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AppFuture {
    state: AppFutureState,
    context: AppContext,
    request: Request<()>,
    app: App,
}

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
#[derive(Debug)]
enum AppFutureState {
    Init,
    InFlight(AsyncResult<Output>),
    Done,
}

macro_rules! input {
    ($self:expr) => {
        &mut Input {
            context: &mut $self.context,
            request: &$self.request,
            app: &$self.app,
            _marker: PhantomData,
        }
    };
}

impl AppFuture {
    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        use self::AppFutureState::*;
        loop {
            self.state = match self.state {
                InFlight(ref mut in_flight) => return in_flight.poll_ready(input!(self)),
                Init => {
                    if let Some(output) = self.app.data.callback.on_init(input!(self))? {
                        return Ok(Async::Ready(output));
                    }

                    let (pos, params) = match self
                        .app
                        .recognize(self.request.uri().path(), self.request.method())
                    {
                        Ok(Recognize::Matched(pos, captures))
                        | Ok(Recognize::FallbackHead(pos, captures)) => (pos, captures),
                        Ok(Recognize::FallbackOptions(allowed_methods)) => {
                            let mut response =
                                http::Response::new(crate::output::ResponseBody::default());
                            response
                                .headers_mut()
                                .insert(http::header::ALLOW, allowed_methods.clone());
                            return Ok(Async::Ready(response));
                        }
                        Err(e) => return Err(e.into()),
                    };
                    let route = &self.app.data.routes[pos];
                    debug_assert_eq!(route.id.1, pos);

                    self.context.route = Some((route.id, params));

                    let mut in_flight = route.handler.handle();
                    for &id in route.modifier_ids.iter().rev() {
                        let scope = self.app.get_scope(id).expect("should be valid ID");
                        in_flight = scope.modifier.modify(in_flight);
                    }

                    InFlight(in_flight)
                }
                Done => panic!("the future has already polled."),
            }
        }
    }

    fn handle_response(&mut self, mut output: Output) -> Result<Output, Critical> {
        if let Some(ref jar) = self.context.cookies {
            // append Cookie entries.
            for cookie in jar.delta() {
                output.headers_mut().append(
                    header::SET_COOKIE,
                    cookie.encoded().to_string().parse().unwrap(),
                );
            }
        }

        if let Some(hdrs) = self.context.response_headers.take() {
            output.headers_mut().extend(hdrs);
        }

        // append the value of Content-Length to the response header if missing.
        if let Some(len) = output.body().content_length() {
            output
                .headers_mut()
                .entry(header::CONTENT_LENGTH)
                .expect("never fails")
                .or_insert_with(|| {
                    // safety: '0'-'9' is ascci.
                    // TODO: more efficient
                    unsafe { HeaderValue::from_shared_unchecked(len.to_string().into()) }
                });
        }

        Ok(output)
    }

    fn handle_error(&mut self, err: Error) -> Result<Output, Critical> {
        self.app.data.callback.on_error(err, input!(self))
    }
}

impl Future for AppFuture {
    type Item = Response<ResponseBody>;
    type Error = Critical;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let output = match ready!(self.poll_in_flight()) {
            Ok(output) => output,
            Err(err) => {
                self.state = AppFutureState::Done;
                self.handle_error(err)?
            }
        };
        self.handle_response(output).map(Async::Ready)
    }
}

#[derive(Debug)]
struct AppContext {
    body: Option<RequestBody>,
    is_upgraded: bool,
    locals: LocalMap,
    response_headers: Option<HeaderMap>,
    cookies: Option<CookieJar>,
    route: Option<(RouteId, Option<Captures>)>,
}

/// A proxy object for accessing the contextual information about incoming HTTP request
/// and global/request-local state.
#[derive(Debug)]
pub struct Input<'task> {
    context: &'task mut AppContext,
    request: &'task Request<()>,
    app: &'task App,
    _marker: PhantomData<Rc<()>>,
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
    pub fn take_body(&mut self) -> Option<RequestBody> {
        self.context.body.take()
    }

    /// Creates an instance of "ReadAll" from the raw message body.
    pub fn read_all(&mut self) -> Option<crate::input::body::ReadAll> {
        self.take_body().map(crate::input::body::ReadAll::new)
    }

    /// Returns 'true' if the upgrade function is set.
    pub fn is_upgraded(&self) -> bool {
        self.context.is_upgraded
    }

    /// Registers the upgrade function to this request.
    #[inline]
    pub fn upgrade<F, R>(&mut self, on_upgrade: F) -> Result<(), F>
    where
        F: FnOnce(crate::input::body::UpgradedIo) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        if self.is_upgraded() {
            return Err(on_upgrade);
        }
        self.context.is_upgraded = true;

        let body = self.take_body().expect("The body has already gone");
        crate::rt::spawn(
            body.on_upgrade()
                .map_err(|_| ())
                .and_then(move |upgraded| on_upgrade(upgraded).into_future()),
        );

        Ok(())
    }

    /// Returns a reference to the parsed value of `Content-type` stored in the specified `Input`.
    pub fn content_type(&mut self) -> Result<Option<&Mime>, Error> {
        use crate::input::local_map::{local_key, Entry};

        local_key! {
            static KEY: Option<Mime>;
        }

        match self.context.locals.entry(&KEY) {
            Entry::Occupied(entry) => Ok(entry.into_mut().as_ref()),
            Entry::Vacant(entry) => {
                let mime = match self.request.headers().get(http::header::CONTENT_TYPE) {
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

    /// Returns a proxy object for accessing parameters extracted by the router.
    pub fn params(&self) -> Option<Params<'_>> {
        let route = self.context.route.as_ref()?;
        Some(Params {
            path: self.request.uri().path(),
            names: self.app.uri(route.0).capture_names()?,
            captures: route.1.as_ref()?,
        })
    }

    /// Returns the reference to a state of `T` registered in the scope, if possible.
    ///
    /// This method will return a `None` if a value of `T` is not registered in the scope.
    #[inline]
    pub fn state<T>(&self) -> Option<State<T>>
    where
        T: Send + Sync + 'static,
    {
        self.app
            .get_state(self.context.route.as_ref()?.0)
            .map(|state| State {
                state,
                _marker: PhantomData,
            })
    }

    /// Returns a proxy object for managing the value of Cookie entries.
    ///
    /// This function will perform parsing when called at first, and returns an `Err`
    /// if the value of header field is invalid.
    pub fn cookies(&mut self) -> Result<Cookies<'_>, Error> {
        if let Some(ref mut jar) = self.context.cookies {
            return Ok(Cookies {
                jar,
                _marker: PhantomData,
            });
        }

        let jar = self.context.cookies.get_or_insert_with(CookieJar::new);

        for raw in self.request.headers().get_all(http::header::COOKIE) {
            let raw_s = raw.to_str().map_err(crate::error::bad_request)?;
            for s in raw_s.split(';').map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)
                    .map_err(crate::error::bad_request)?
                    .into_owned();
                jar.add_original(cookie);
            }
        }

        Ok(Cookies {
            jar,
            _marker: PhantomData,
        })
    }

    /// Returns a reference to `LocalMap` for managing request-local data.
    #[cfg_attr(tarpaulin, skip)]
    #[inline]
    pub fn locals(&self) -> &LocalMap {
        &self.context.locals
    }

    /// Returns a mutable reference to `LocalMap` for managing request-local data.
    #[cfg_attr(tarpaulin, skip)]
    #[inline]
    pub fn locals_mut(&mut self) -> &mut LocalMap {
        &mut self.context.locals
    }

    /// Returns a mutable reference to a map that holds the additional header fields inserted into the response.
    pub fn response_headers(&mut self) -> &mut HeaderMap {
        self.context
            .response_headers
            .get_or_insert_with(Default::default)
    }
}

/// A proxy object for accessing the certain state.
#[derive(Debug)]
pub struct State<T>
where
    T: Send + Sync + 'static,
{
    state: *const T,
    _marker: PhantomData<UnsafeCell<()>>,
}

impl<T> State<T>
where
    T: Send + Sync + 'static,
{
    /// Acquires a reference to the associated state.
    ///
    /// If the reference to `Input` is not set on the current task context,
    /// this method will return a `None`.
    #[inline]
    pub fn get(&self) -> Option<&T> {
        if crate::input::is_set_current() {
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    /// Gets a reference to the associated state without checking the task context.
    ///
    /// # Safety
    /// This method assumes that the reference to `Input` is set on the current task context.
    #[inline]
    pub unsafe fn get_unchecked(&self) -> &T {
        &*self.state
    }
}

unsafe impl<T> Send for State<T> where T: Send + Sync + 'static {}

/// A proxy object for accessing Cookie values.
///
/// Currently this type is a thin wrapper of `&mut cookie::CookieJar`.
#[derive(Debug)]
pub struct Cookies<'a> {
    jar: &'a mut CookieJar,
    _marker: PhantomData<Rc<()>>,
}

impl<'a> Cookies<'a> {
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

/// A proxy object for accessing extracted parameters.
#[derive(Debug)]
pub struct Params<'input> {
    path: &'input str,
    names: &'input CaptureNames,
    captures: &'input Captures,
}

impl<'input> Params<'input> {
    /// Returns `true` if the extracted paramater exists.
    pub fn is_empty(&self) -> bool {
        self.captures.params().is_empty() && self.captures.wildcard().is_none()
    }

    /// Returns the value of `i`-th parameter, if exists.
    pub fn get(&self, i: usize) -> Option<&str> {
        let &(s, e) = self.captures.params().get(i)?;
        self.path.get(s..e)
    }

    /// Returns the value of wildcard parameter, if exists.
    pub fn get_wildcard(&self) -> Option<&str> {
        let (s, e) = self.captures.wildcard()?;
        self.path.get(s..e)
    }

    /// Returns the value of parameter whose name is equal to `name`, if exists.
    pub fn name(&self, name: &str) -> Option<&str> {
        match name {
            "*" => self.get_wildcard(),
            name => self.get(self.names.get_position(name)?),
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
