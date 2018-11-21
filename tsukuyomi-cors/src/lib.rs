//! CORS support for Tsukuyomi.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-cors/0.1.0")]
#![warn(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]
#![cfg_attr(feature = "cargo-clippy", allow(stutter))]
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate failure;
extern crate http;
extern crate tsukuyomi;

use {
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
        Method, Request, Response, StatusCode, Uri,
    },
    std::{collections::HashSet, sync::Arc, time::Duration},
    tsukuyomi::{
        extractor::{Extract, ExtractStatus, Extractor}, //
        handler::AsyncResult,
        HttpError,
        Input,
        Modifier,
        Output,
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
    pub fn allow_origin(mut self, origin: impl Into<Uri>) -> Self {
        self.origins
            .get_or_insert_with(Default::default)
            .insert(origin.into());
        self
    }

    #[allow(missing_docs)]
    pub fn allow_origins(mut self, origins: impl IntoIterator<Item = Uri>) -> Self {
        self.origins
            .get_or_insert_with(Default::default)
            .extend(origins);
        self
    }

    #[allow(missing_docs)]
    pub fn allow_method(mut self, method: Method) -> Self {
        self.methods
            .get_or_insert_with(Default::default)
            .insert(method);
        self
    }

    #[allow(missing_docs)]
    pub fn allow_methods(mut self, methods: impl IntoIterator<Item = Method>) -> Self {
        self.methods
            .get_or_insert_with(Default::default)
            .extend(methods);
        self
    }

    #[allow(missing_docs)]
    pub fn allow_header(mut self, header: HeaderName) -> Self {
        self.headers
            .get_or_insert_with(Default::default)
            .insert(header);
        self
    }

    #[allow(missing_docs)]
    pub fn allow_headers(mut self, headers: impl IntoIterator<Item = HeaderName>) -> Self {
        self.headers
            .get_or_insert_with(Default::default)
            .extend(headers);
        self
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
                }).into(),
        ).expect("should be a valid header value");

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
                    }).into(),
            ).expect("should be a valid header value")
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
}

impl Extractor for CORS {
    type Output = ();
    type Error = tsukuyomi::Error;
    type Future = tsukuyomi::extractor::Placeholder<Self::Output, Self::Error>;

    fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
        match self.inner.process_request(input)? {
            Some(output) => Ok(ExtractStatus::Canceled(output)),
            None => Ok(ExtractStatus::Ready(())),
        }
    }
}

impl Modifier for CORS {
    fn modify(&self, mut handle: AsyncResult<Output>) -> AsyncResult<Output> {
        let inner = self.inner.clone();
        let mut cors_handled = false;

        AsyncResult::poll_fn(move |input| {
            if !cors_handled {
                cors_handled = true;
                if let Some(output) = inner.process_request(input)? {
                    return Ok(output.into());
                }
            }
            handle.poll_ready(input)
        })
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
    fn validate_origin(&self, origin: &HeaderValue) -> Result<AllowedOrigin, CORSError> {
        let parsed_origin = parse_origin(origin)?;

        if let Some(ref origins) = self.origins {
            if !origins.contains(&parsed_origin) {
                return Err(CORSErrorKind::DisallowedOrigin.into());
            }
            return Ok(AllowedOrigin::Some(origin.clone()));
        }

        if self.allow_credentials {
            Ok(AllowedOrigin::Some(origin.clone()))
        } else {
            Ok(AllowedOrigin::Any)
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

    fn process_request(&self, input: &mut Input<'_>) -> tsukuyomi::Result<Option<Output>> {
        let origin = match input.request.headers().get(ORIGIN) {
            Some(origin) => self.validate_origin(origin)?,
            None => return Ok(None),
        };

        if input.request.method() == Method::OPTIONS {
            self.process_preflight_request(input.request, origin)
                .map(|response| Some(response.map(Into::into)))
                .map_err(Into::into)
        } else {
            let response_headers = input.response_headers();
            self.process_simple_request(input.request, origin, response_headers)
                .map(|_| None)
                .map_err(Into::into)
        }
    }
}

fn parse_origin(h: &HeaderValue) -> Result<Uri, CORSError> {
    let h_str = h.to_str().map_err(|_| CORSErrorKind::InvalidOrigin)?;
    let origin_uri: Uri = h_str.parse().map_err(|_| CORSErrorKind::InvalidOrigin)?;

    if origin_uri.scheme_part().is_none() {
        return Err(CORSErrorKind::InvalidOrigin.into());
    }

    if origin_uri.host().is_none() {
        return Err(CORSErrorKind::InvalidOrigin.into());
    }

    Ok(origin_uri)
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

#[derive(Debug, Fail)]
#[fail(display = "Invalid CORS request: {}", kind)]
struct CORSError {
    kind: CORSErrorKind,
}

impl From<CORSErrorKind> for CORSError {
    fn from(kind: CORSErrorKind) -> Self {
        Self { kind }
    }
}

impl HttpError for CORSError {
    fn status_code(&self) -> StatusCode {
        StatusCode::FORBIDDEN
    }
}

#[derive(Debug, Fail)]
enum CORSErrorKind {
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
