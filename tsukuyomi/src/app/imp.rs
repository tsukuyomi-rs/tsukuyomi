use {
    super::{App, RouteId},
    cookie::{Cookie, CookieJar},
    crate::{
        error::{Critical, Error, HttpError},
        handler::AsyncResult,
        input::RequestBody,
        localmap::LocalMap,
        output::{Output, ResponseBody},
        recognizer::Captures,
        uri::CaptureNames,
    },
    futures::{Async, Future, IntoFuture, Poll},
    http::{
        header::{self, HeaderMap, HeaderValue},
        Method, Request, Response, StatusCode,
    },
    hyper::body::Payload,
    mime::Mime,
    std::{cell::Cell, marker::PhantomData, mem, ops::Index, ptr::NonNull, rc::Rc},
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
            request: Request::from_parts(parts, ()),
            app: self.clone(),
            body: BodyState::Some(body),
            cookie_jar: None,
            locals: LocalMap::default(),
            response_headers: None,
            state: AppFutureState::Init,
        }
    }
}

/// A future for managing an incoming HTTP request, created by `AppService`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AppFuture {
    request: Request<()>,
    app: App,
    body: BodyState,
    cookie_jar: Option<CookieJar>,
    locals: LocalMap,
    response_headers: Option<HeaderMap>,
    state: AppFutureState,
}

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
#[derive(Debug)]
enum AppFutureState {
    Init,
    Handle {
        in_flight: AsyncResult<Output>,
        route: (RouteId, Option<Captures>),
    },
    Done,
}

#[derive(Debug)]
enum BodyState {
    Some(RequestBody),
    Gone,
    Upgraded,
}

macro_rules! input {
    ($self:expr) => {
        input!($self, None)
    };
    ($self:expr, $route:expr) => {
        &mut Input {
            request: &$self.request,
            params: {
                let path = $self.request.uri().path();
                let app = &$self.app;
                &$route.and_then(|route: &(RouteId, Option<Captures>)| {
                    Some(Params {
                        path,
                        names: app.uri(route.0).capture_names()?,
                        captures: route.1.as_ref()?,
                    })
                })
            },
            states: &States {
                app: &$self.app,
                route_id: $route.map(|route: &(RouteId, Option<Captures>)| route.0),
            },
            cookies: &mut Cookies {
                jar: &mut $self.cookie_jar,
                request_headers: &$self.request.headers(),
                _marker: PhantomData,
            },
            locals: &mut $self.locals,
            response_headers: &mut $self.response_headers,
            body: &mut $self.body,
            _marker: PhantomData,
        }
    };
}

impl AppFuture {
    fn poll_in_flight(&mut self) -> Poll<Output, Error> {
        use self::AppFutureState::*;
        loop {
            self.state = match self.state {
                Handle {
                    ref mut in_flight,
                    ref route,
                } => return in_flight.poll_ready(input!(self, Some(route))),
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

                    let mut in_flight = route.handler.handle();
                    for &id in route.modifier_ids.iter().rev() {
                        let scope = self.app.get_scope(id).expect("should be valid ID");
                        in_flight = scope.modifier.modify(in_flight);
                    }

                    Handle {
                        in_flight,
                        route: (route.id, params),
                    }
                }
                Done => panic!("the future has already polled."),
            }
        }
    }

    fn handle_response(&mut self, mut output: Output) -> Result<Output, Critical> {
        // append Cookie entries.
        if let Some(ref jar) = self.cookie_jar {
            for cookie in jar.delta() {
                output.headers_mut().append(
                    header::SET_COOKIE,
                    cookie.encoded().to_string().parse().unwrap(),
                );
            }
        }

        // append response headers.
        if let Some(hdrs) = self.response_headers.take() {
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
pub struct States<'task> {
    app: &'task App,
    route_id: Option<RouteId>,
}

impl<'task> States<'task> {
    /// Returns the reference to a shared state of `T` registered in the scope.
    #[inline]
    pub fn try_get<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.app.get_state(self.route_id?)
    }

    /// Returns the reference to a shared state of `T` registered in the scope.
    ///
    /// # Panics
    /// This method will panic if the state of `T` is not registered in the scope.
    #[inline]
    pub fn get<T>(&self) -> &T
    where
        T: Send + Sync + 'static,
    {
        self.try_get().expect("The state is not set")
    }
}

/// A proxy object for accessing Cookie values.
#[derive(Debug)]
pub struct Cookies<'task> {
    jar: &'task mut Option<CookieJar>,
    request_headers: &'task HeaderMap,
    _marker: PhantomData<Rc<()>>,
}

impl<'task> Cookies<'task> {
    /// Returns the mutable reference to the inner `CookieJar` if available.
    pub fn jar(&mut self) -> crate::error::Result<&mut CookieJar> {
        if let Some(ref mut jar) = self.jar {
            return Ok(jar);
        }

        let jar = self.jar.get_or_insert_with(CookieJar::new);

        for raw in self.request_headers.get_all(http::header::COOKIE) {
            let raw_s = raw.to_str().map_err(crate::error::bad_request)?;
            for s in raw_s.split(';').map(|s| s.trim()) {
                let cookie = Cookie::parse_encoded(s)
                    .map_err(crate::error::bad_request)?
                    .into_owned();
                jar.add_original(cookie);
            }
        }

        Ok(jar)
    }
}

#[cfg(feature = "secure")]
mod secure {
    use cookie::{Key, PrivateJar, SignedJar};
    use crate::error::Result;

    impl<'a> super::Cookies<'a> {
        /// Creates a `SignedJar` with the specified secret key.
        #[inline]
        pub fn signed_jar(&mut self, key: &Key) -> Result<SignedJar<'_>> {
            Ok(self.jar()?.signed(key))
        }

        /// Creates a `PrivateJar` with the specified secret key.
        #[inline]
        pub fn private_jar(&mut self, key: &Key) -> Result<PrivateJar<'_>> {
            Ok(self.jar()?.private(key))
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

/// A proxy object for accessing the contextual information about incoming HTTP request
/// and global/request-local state.
#[derive(Debug)]
pub struct Input<'task> {
    /// The information of incoming request without the message body.
    pub request: &'task Request<()>,

    /// A set of extracted parameters from router.
    pub params: &'task Option<Params<'task>>,

    /// A proxy object for accessing shared states.
    pub states: &'task States<'task>,

    /// A proxy object for accessing Cookie values.
    pub cookies: &'task mut Cookies<'task>,

    /// A typemap that holds arbitrary request-local data.
    pub locals: &'task mut LocalMap,

    /// A header map that holds additional response header fields.
    pub response_headers: &'task mut Option<HeaderMap>,

    body: &'task mut BodyState,
    _marker: PhantomData<Rc<()>>,
}

impl<'task> Input<'task> {
    /// Stores this reference to the task local storage and executes the specified closure.
    ///
    /// The stored reference to `Input` can be accessed by using `input::with_get_current`.
    #[inline]
    pub fn with_set_current<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        with_set_current(self, f)
    }

    /// Takes a raw instance of incoming message body from the context.
    pub fn body(&mut self) -> Option<RequestBody> {
        match mem::replace(self.body, BodyState::Gone) {
            BodyState::Some(body) => Some(body),
            _ => None,
        }
    }

    /// Registers the upgrade handler to the context.
    #[inline]
    pub fn upgrade<F, R>(&mut self, on_upgrade: F) -> Result<(), F>
    where
        F: FnOnce(crate::input::body::UpgradedIo) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        let body = match mem::replace(self.body, BodyState::Upgraded) {
            BodyState::Some(body) => body,
            _ => return Err(on_upgrade),
        };

        crate::rt::spawn(
            body.on_upgrade()
                .map_err(|_| ())
                .and_then(move |upgraded| on_upgrade(upgraded).into_future()),
        );

        Ok(())
    }

    /// Returns 'true' if the context has already upgraded.
    pub fn is_upgraded(&self) -> bool {
        match self.body {
            BodyState::Upgraded => true,
            _ => false,
        }
    }

    /// Parses the header field `Content-type` and stores it into the localmap.
    pub fn content_type(&mut self) -> Result<Option<&Mime>, Error> {
        use crate::localmap::{local_key, Entry};

        local_key! {
            static KEY: Option<Mime>;
        }

        match self.locals.entry(&KEY) {
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
fn with_set_current<R>(self_: &mut Input<'_>, f: impl FnOnce() -> R) -> R {
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
