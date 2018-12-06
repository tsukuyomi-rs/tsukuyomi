//! Components for constructing HTTP applications.

pub mod fallback;
pub mod route;

mod builder;
mod error;
mod mount;
mod recognizer;
mod service;
mod uri;

#[cfg(test)]
mod tests;

pub use self::{
    builder::{Builder, Scope},
    error::{Error, Result},
    mount::{mount, Mount},
    service::AppService,
};

pub(crate) use self::{recognizer::Captures, uri::CaptureNames};

use {
    self::{
        fallback::Fallback,
        recognizer::{RecognizeError, Recognizer},
        uri::Uri,
    },
    crate::handler::BoxedHandler,
    crate::{core::TryFrom, error::Critical, input::body::RequestBody, output::ResponseBody},
    bytes::BytesMut,
    http::header::HeaderValue,
    http::Method,
    http::{Request, Response},
    indexmap::{IndexMap, IndexSet},
    std::fmt,
    std::sync::Arc,
    tower_service::NewService,
};

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    inner: Arc<AppInner>,
}

struct AppInner {
    recognizer: Recognizer<Resource>,
    global_fallback: Option<Arc<Box<dyn Fallback + Send + Sync + 'static>>>,
}

impl fmt::Debug for AppInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppInner")
            .field("recognizer", &self.recognizer)
            .finish()
    }
}

impl AppInner {
    fn resource(&self, id: ResourceId) -> &Resource {
        self.recognizer.get(id.0).expect("the wrong resource ID")
    }

    fn route(&self, path: &str, method: &Method) -> Route<'_> {
        let mut captures = None;
        let resource = match self.recognizer.recognize(path, &mut captures) {
            Ok(resource) => resource,
            Err(RecognizeError::NotMatched) => {
                return Route::NotFound {
                    resources: vec![],
                    captures,
                };
            }
            Err(RecognizeError::PartiallyMatched(candidates)) => {
                return Route::NotFound {
                    resources: candidates
                        .iter()
                        .filter_map(|i| self.recognizer.get(i))
                        .collect(),
                    captures,
                };
            }
        };

        if let Some(endpoint) = resource.recognize(method) {
            Route::FoundEndpoint {
                endpoint,
                resource,
                captures,
            }
        } else {
            Route::FoundResource { resource, captures }
        }
    }
}

impl App {
    /// Create a `Builder` to configure the instance of `App`.
    pub fn builder() -> Builder<()> {
        Builder::default()
    }

    /// Create a `Builder` with the specified prefix.
    pub fn with_prefix<T>(prefix: T) -> Result<Builder<()>>
    where
        Uri: TryFrom<T>,
    {
        Ok(Self::builder().prefix(Uri::try_from(prefix)?))
    }
}

impl NewService for App {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = Critical;
    type Service = AppService;
    type InitError = Critical;
    type Future = futures01::future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        futures01::future::ok(AppService {
            inner: self.inner.clone(),
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct ResourceId(usize);

/// A type representing a set of endpoints with the same HTTP path.
pub struct Resource {
    id: ResourceId,
    uri: Uri,
    endpoints: Vec<Endpoint>,
    fallback: Option<Arc<Box<dyn Fallback + Send + Sync + 'static>>>,
    allowed_methods: IndexMap<Method, usize>,
    allowed_methods_value: HeaderValue,
}

impl fmt::Debug for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resource")
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("endpoints", &self.endpoints)
            .field("fallback", &self.fallback.as_ref().map(|_| "<fallback>"))
            .field("allowed_methods", &self.allowed_methods)
            .field("allowed_methods_value", &self.allowed_methods_value)
            .finish()
    }
}

impl Resource {
    pub fn allowed_methods<'a>(&'a self) -> impl Iterator<Item = &'a Method> + 'a {
        self.allowed_methods.keys()
    }

    fn recognize(&self, method: &Method) -> Option<&Endpoint> {
        self.allowed_methods
            .get(method)
            .map(|&pos| &self.endpoints[pos])
    }

    fn update(&mut self) {
        self.allowed_methods_value = {
            let allowed_methods: IndexSet<_> = self
                .allowed_methods
                .keys()
                .chain(Some(&Method::OPTIONS))
                .collect();
            let bytes =
                allowed_methods
                    .iter()
                    .enumerate()
                    .fold(BytesMut::new(), |mut acc, (i, m)| {
                        if i > 0 {
                            acc.extend_from_slice(b", ");
                        }
                        acc.extend_from_slice(m.as_str().as_bytes());
                        acc
                    });
            unsafe { HeaderValue::from_shared_unchecked(bytes.freeze()) }
        };
    }
}

/// A struct representing a set of data associated with an endpoint.
#[doc(hidden)]
pub struct Endpoint {
    id: usize,
    uri: Uri,
    methods: IndexSet<Method>,
    handler: BoxedHandler,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("methods", &self.methods)
            .finish()
    }
}

#[derive(Debug)]
enum Route<'a> {
    /// The URI is matched and a route associated with the specified method is found.
    FoundEndpoint {
        endpoint: &'a Endpoint,
        resource: &'a Resource,
        captures: Option<Captures>,
    },

    /// the URI is matched, but the method is disallowed.
    FoundResource {
        resource: &'a Resource,
        captures: Option<Captures>,
    },

    /// The URI is not matched to any endpoints.
    NotFound {
        resources: Vec<&'a Resource>,
        captures: Option<Captures>,
    },
}