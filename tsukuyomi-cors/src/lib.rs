//! CORS support for Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-cors/0.2.0-dev")]
#![deny(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![forbid(clippy::unimplemented)]

use {
    either::Either,
    failure::Fail,
    http::{
        header::{
            HeaderMap, //
            HeaderName,
            HeaderValue,
            ACCESS_CONTROL_ALLOW_CREDENTIALS,
            ACCESS_CONTROL_ALLOW_HEADERS,
            ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
            ACCESS_CONTROL_MAX_AGE,
            ACCESS_CONTROL_REQUEST_HEADERS,
            ACCESS_CONTROL_REQUEST_METHOD,
            ORIGIN,
        },
        HttpTryFrom, Method, Request, Response, StatusCode, Uri,
    },
    std::{collections::HashSet, sync::Arc, time::Duration},
    tsukuyomi::{
        app::config::{AppConfig, AppConfigContext},
        core::Chain,
        future::MaybeFuture,
        handler::{Handler, ModifyHandler},
        HttpError, Input, Output, Responder,
    },
};

/// A builder of `CORS`.
#[derive(Debug, Default)]
pub struct Builder {
    origins: Option<HashSet<Uri>>,
    methods: Option<HashSet<Method>>,
    headers: Option<HashSet<HeaderName>>,
    max_age: Option<Duration>,
    allow_credentials: bool,
}

impl Builder {
    /// Creates a `Builder` with the default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(missing_docs)]
    pub fn allow_origin<U>(mut self, origin: U) -> http::Result<Self>
    where
        Uri: HttpTryFrom<U>,
    {
        let origin = Uri::try_from(origin).map_err(Into::into)?;
        self.origins
            .get_or_insert_with(Default::default)
            .insert(origin);
        Ok(self)
    }

    #[allow(missing_docs)]
    pub fn allow_origins<U>(mut self, origins: impl IntoIterator<Item = U>) -> http::Result<Self>
    where
        Uri: HttpTryFrom<U>,
    {
        let origins = origins
            .into_iter()
            .map(Uri::try_from)
            .collect::<Result<Vec<Uri>, _>>()
            .map_err(Into::into)?;
        self.origins
            .get_or_insert_with(Default::default)
            .extend(origins);
        Ok(self)
    }

    #[allow(missing_docs)]
    pub fn allow_method<M>(mut self, method: M) -> http::Result<Self>
    where
        Method: HttpTryFrom<M>,
    {
        let method = Method::try_from(method).map_err(Into::into)?;
        self.methods
            .get_or_insert_with(Default::default)
            .insert(method);
        Ok(self)
    }

    #[allow(missing_docs)]
    pub fn allow_methods<M>(mut self, methods: impl IntoIterator<Item = M>) -> http::Result<Self>
    where
        Method: HttpTryFrom<M>,
    {
        let methods = methods
            .into_iter()
            .map(Method::try_from)
            .collect::<Result<Vec<Method>, _>>()
            .map_err(Into::into)?;
        self.methods
            .get_or_insert_with(Default::default)
            .extend(methods);
        Ok(self)
    }

    #[allow(missing_docs)]
    pub fn allow_header<H>(mut self, header: H) -> http::Result<Self>
    where
        HeaderName: HttpTryFrom<H>,
    {
        let header = HeaderName::try_from(header).map_err(Into::into)?;
        self.headers
            .get_or_insert_with(Default::default)
            .insert(header);
        Ok(self)
    }

    #[allow(missing_docs)]
    pub fn allow_headers<H>(mut self, headers: impl IntoIterator<Item = H>) -> http::Result<Self>
    where
        HeaderName: HttpTryFrom<H>,
    {
        let headers = headers
            .into_iter()
            .map(HeaderName::try_from)
            .collect::<Result<Vec<HeaderName>, _>>()
            .map_err(Into::into)?;
        self.headers
            .get_or_insert_with(Default::default)
            .extend(headers);
        Ok(self)
    }

    #[allow(missing_docs)]
    pub fn allow_credentials(self, enabled: bool) -> Self {
        Self {
            allow_credentials: enabled,
            ..self
        }
    }

    #[allow(missing_docs)]
    pub fn max_age(self, max_age: Duration) -> Self {
        Self {
            max_age: Some(max_age),
            ..self
        }
    }

    #[allow(missing_docs)]
    pub fn build(self) -> CORS {
        let methods = self.methods.unwrap_or_else(|| {
            vec![Method::GET, Method::POST, Method::OPTIONS]
                .into_iter()
                .collect()
        });

        let methods_value = HeaderValue::from_shared(
            methods
                .iter()
                .enumerate()
                .fold(String::new(), |mut acc, (i, m)| {
                    if i > 0 {
                        acc += ",";
                    }
                    acc += m.as_str();
                    acc
                })
                .into(),
        )
        .expect("should be a valid header value");

        let headers_value = self.headers.as_ref().map(|hdrs| {
            HeaderValue::from_shared(
                hdrs.iter()
                    .enumerate()
                    .fold(String::new(), |mut acc, (i, hdr)| {
                        if i > 0 {
                            acc += ",";
                        }
                        acc += hdr.as_str();
                        acc
                    })
                    .into(),
            )
            .expect("should be a valid header value")
        });

        CORS {
            inner: Arc::new(Inner {
                origins: self.origins,
                methods,
                methods_value,
                headers: self.headers,
                headers_value,
                max_age: self.max_age,
                allow_credentials: self.allow_credentials,
            }),
        }
    }
}

/// The main type for providing the CORS filtering.
#[derive(Debug, Clone)]
pub struct CORS {
    inner: Arc<Inner>,
}

impl Default for CORS {
    fn default() -> Self {
        Self::new()
    }
}

impl CORS {
    /// Create a new `CORS` with the default configuration.
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Create a builder of this type.
    pub fn builder() -> Builder {
        Builder::new()
    }

    #[allow(missing_docs)]
    pub fn wrap_scope<S>(&self, scope: S) -> CORSScope<S> {
        CORSScope {
            cors: self.clone(),
            scope,
        }
    }
}

/// The implementation of `Scope` for registering itself as `Modifier` and `Fallback`
/// into a specific scope.
#[derive(Debug)]
pub struct CORSScope<S> {
    cors: CORS,
    scope: S,
}

impl<M, S> AppConfig<M> for CORSScope<S>
where
    M: ModifyHandler<CORS> + Clone,
    M::Handler: Send + Sync + 'static,
    M::Output: Responder,
    S: AppConfig<Chain<M, CORS>>,
{
    type Error = tsukuyomi::app::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        cx.set_default_handler(self.cors.clone())?;
        self.scope
            .configure(&mut cx.with_modifier(self.cors))
            .map_err(Into::into)?;
        Ok(())
    }
}

/// The implementation of `Handler` for processing the CORS request.
///
/// This fallback adds the processing for CORS preflight request for all URLs
/// registered in the scope. If the route explicitly handles `OPTIONS`, it will be
/// ignored.
impl Handler for CORS {
    type Output = Response<()>;
    type Future = tsukuyomi::future::NeverFuture<Self::Output, tsukuyomi::Error>;

    fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        if input.request.uri().path() != "*" || input.request.method() != Method::OPTIONS {
            return MaybeFuture::err(StatusCode::NOT_FOUND.into());
        }

        match self.inner.validate_origin(input.request) {
            Ok(Some(origin)) => MaybeFuture::Ready(
                self.inner
                    .process_preflight_request(input.request, origin)
                    .map_err(Into::into),
            ),
            Ok(None) => MaybeFuture::err(StatusCode::NOT_FOUND.into()),
            Err(err) => MaybeFuture::err(err.into()),
        }
    }
}

/// The implementation of `Modifier` for processing CORS requests.
///
/// This modifier inserts the processing of CORS request for all `AsyncResult`s
/// returned from the handlers in the scope.
impl<H> ModifyHandler<H> for CORS
where
    H: Handler,
    H::Output: 'static,
{
    type Output = Either<Output, H::Output>;
    type Handler = CORSHandler<H>;

    fn modify(&self, handler: H) -> Self::Handler {
        CORSHandler {
            handler,
            inner: self.inner.clone(),
        }
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct CORSHandler<H> {
    handler: H,
    inner: Arc<Inner>,
}

#[allow(clippy::type_complexity)]
impl<H> Handler for CORSHandler<H>
where
    H: Handler,
    H::Output: 'static,
{
    type Output = Either<Output, H::Output>;
    type Future = tsukuyomi::future::MapResult<
        H::Future,
        fn(
            Result<H::Output, <H::Future as tsukuyomi::future::Future>::Error>,
        ) -> Result<Self::Output, tsukuyomi::error::Error>,
    >;

    #[inline]
    fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        match self.inner.process_request(input) {
            Ok(Some(output)) => MaybeFuture::ok(Either::Left(output)),
            Ok(None) => self
                .handler
                .call(input)
                .map_result(|res| res.map(Either::Right).map_err(Into::into)),
            Err(err) => MaybeFuture::err(err.into()),
        }
    }
}

#[derive(Debug)]
struct Inner {
    origins: Option<HashSet<Uri>>,
    methods: HashSet<Method>,
    methods_value: HeaderValue,
    headers: Option<HashSet<HeaderName>>,
    headers_value: Option<HeaderValue>,
    max_age: Option<Duration>,
    allow_credentials: bool,
}

impl Inner {
    fn validate_origin<T>(&self, request: &Request<T>) -> Result<Option<AllowedOrigin>, CORSError> {
        let origin = match request.headers().get(ORIGIN) {
            Some(origin) => origin,
            None => return Ok(None),
        };

        let parsed_origin = {
            let h_str = origin.to_str().map_err(|_| CORSErrorKind::InvalidOrigin)?;
            let origin_uri: Uri = h_str.parse().map_err(|_| CORSErrorKind::InvalidOrigin)?;

            if origin_uri.scheme_part().is_none() {
                return Err(CORSErrorKind::InvalidOrigin.into());
            }

            if origin_uri.host().is_none() {
                return Err(CORSErrorKind::InvalidOrigin.into());
            }

            origin_uri
        };

        if let Some(ref origins) = self.origins {
            if !origins.contains(&parsed_origin) {
                return Err(CORSErrorKind::DisallowedOrigin.into());
            }
            return Ok(Some(AllowedOrigin::Some(origin.clone())));
        }

        if self.allow_credentials {
            Ok(Some(AllowedOrigin::Some(origin.clone())))
        } else {
            Ok(Some(AllowedOrigin::Any))
        }
    }

    fn validate_request_method<T>(
        &self,
        request: &Request<T>,
    ) -> Result<Option<HeaderValue>, CORSError> {
        match request.headers().get(ACCESS_CONTROL_REQUEST_METHOD) {
            Some(h) => {
                let method: Method = h
                    .to_str()
                    .map_err(|_| CORSErrorKind::InvalidRequestMethod)?
                    .parse()
                    .map_err(|_| CORSErrorKind::InvalidRequestMethod)?;
                if self.methods.contains(&method) {
                    Ok(Some(self.methods_value.clone()))
                } else {
                    Err(CORSErrorKind::DisallowedRequestMethod.into())
                }
            }
            None => Ok(None),
        }
    }

    fn validate_request_headers<T>(
        &self,
        request: &Request<T>,
    ) -> Result<Option<HeaderValue>, CORSError> {
        match request.headers().get(ACCESS_CONTROL_REQUEST_HEADERS) {
            Some(hdrs) => match self.headers {
                Some(ref headers) => {
                    let mut request_headers = HashSet::new();
                    let hdrs_str = hdrs
                        .to_str()
                        .map_err(|_| CORSErrorKind::InvalidRequestHeaders)?;
                    for hdr in hdrs_str.split(',').map(|s| s.trim()) {
                        let hdr: HeaderName = hdr
                            .parse()
                            .map_err(|_| CORSErrorKind::InvalidRequestHeaders)?;
                        request_headers.insert(hdr);
                    }

                    if !headers.is_superset(&request_headers) {
                        return Err(CORSErrorKind::DisallowedRequestHeaders.into());
                    }

                    Ok(self.headers_value.clone())
                }
                None => Ok(Some(hdrs.clone())),
            },
            None => Ok(None),
        }
    }

    fn process_preflight_request<T>(
        &self,
        request: &Request<T>,
        origin: AllowedOrigin,
    ) -> Result<Response<()>, CORSError> {
        let allow_methods = self.validate_request_method(request)?;
        let allow_headers = self.validate_request_headers(request)?;

        let mut response = Response::default();
        *response.status_mut() = StatusCode::NO_CONTENT;
        response
            .headers_mut()
            .insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin.into());

        if let Some(allow_methods) = allow_methods {
            response
                .headers_mut()
                .insert(ACCESS_CONTROL_ALLOW_METHODS, allow_methods);
        }

        if let Some(allow_headers) = allow_headers {
            response
                .headers_mut()
                .insert(ACCESS_CONTROL_ALLOW_HEADERS, allow_headers);
        }

        if let Some(max_age) = self.max_age {
            response
                .headers_mut()
                .insert(ACCESS_CONTROL_MAX_AGE, max_age.as_secs().into());
        }

        Ok(response)
    }

    fn process_simple_request<T>(
        &self,
        request: &Request<T>,
        origin: AllowedOrigin,
        hdrs: &mut HeaderMap,
    ) -> Result<(), CORSError> {
        if !self.methods.contains(request.method()) {
            return Err(CORSErrorKind::DisallowedRequestMethod.into());
        }

        hdrs.append(ACCESS_CONTROL_ALLOW_ORIGIN, origin.into());

        if self.allow_credentials {
            hdrs.append(
                ACCESS_CONTROL_ALLOW_CREDENTIALS,
                HeaderValue::from_static("true"),
            );
        }

        Ok(())
    }

    fn process_request(&self, input: &mut Input<'_>) -> Result<Option<Output>, CORSError> {
        let origin = match self.validate_origin(input.request)? {
            Some(origin) => origin,
            None => return Ok(None), // do nothing
        };
        if input.request.method() == Method::OPTIONS {
            self.process_preflight_request(input.request, origin)
                .map(|response| Some(response.map(Into::into)))
                .map_err(Into::into)
        } else {
            let response_headers = input.response_headers.get_or_insert_with(Default::default);
            self.process_simple_request(input.request, origin, response_headers)
                .map(|_| None)
                .map_err(Into::into)
        }
    }
}

#[derive(Debug, Clone)]
enum AllowedOrigin {
    Some(HeaderValue),
    Any,
}

impl Into<HeaderValue> for AllowedOrigin {
    fn into(self) -> HeaderValue {
        match self {
            AllowedOrigin::Some(v) => v,
            AllowedOrigin::Any => HeaderValue::from_static("*"),
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug, Fail)]
#[fail(display = "Invalid CORS request: {}", kind)]
pub struct CORSError {
    kind: CORSErrorKind,
}

impl CORSError {
    #[allow(missing_docs)]
    pub fn kind(&self) -> &CORSErrorKind {
        &self.kind
    }
}

impl From<CORSErrorKind> for CORSError {
    fn from(kind: CORSErrorKind) -> Self {
        Self { kind }
    }
}

impl HttpError for CORSError {
    type Body = String;

    fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
        Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(self.to_string())
            .expect("should be a valid response")
    }
}

#[allow(missing_docs)]
#[derive(Debug, Fail)]
pub enum CORSErrorKind {
    #[fail(display = "the provided Origin is not a valid value.")]
    InvalidOrigin,

    #[fail(display = "the provided Origin is not allowed.")]
    DisallowedOrigin,

    #[fail(display = "the provided Access-Control-Request-Method is not a valid value.")]
    InvalidRequestMethod,

    #[fail(display = "the provided Access-Control-Request-Method is not allowed.")]
    DisallowedRequestMethod,

    #[fail(display = "the provided Access-Control-Request-Headers is not a valid value.")]
    InvalidRequestHeaders,

    #[fail(display = "the provided Access-Control-Request-Headers is not allowed.")]
    DisallowedRequestHeaders,
}
