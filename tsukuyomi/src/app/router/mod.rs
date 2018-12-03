mod recognizer;

use {
    self::recognizer::RecognizeError,
    super::{fallback::Fallback, Uri},
    bytes::BytesMut,
    crate::handler::BoxedHandler,
    http::{header::HeaderValue, Method},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
};

pub(super) use self::recognizer::{Captures, Recognizer};

#[derive(Debug)]
pub(super) struct Router {
    pub(super) recognizer: Recognizer<Resource>,
    pub(super) config: Config,
}

impl Router {
    pub(super) fn resource(&self, id: ResourceId) -> &Resource {
        self.recognizer.get(id.0).expect("the wrong resource ID")
    }

    pub(super) fn route(&self, path: &str, method: &Method) -> Route<'_> {
        let mut captures = None;
        let resource = match self.recognizer.recognize(path, &mut captures) {
            Ok(resource) => resource,
            Err(RecognizeError::NotMatched) => return Route::NotFound,
            Err(RecognizeError::PartiallyMatched(_candidates)) => return Route::NotFound,
        };

        if let Some(endpoint) = resource.recognize(method) {
            return Route::Matched {
                endpoint,
                resource,
                captures,
                fallback_head: false,
            };
        }

        if self.config.fallback_head && *method == Method::HEAD {
            if let Some(endpoint) = resource.recognize(&Method::GET) {
                return Route::Matched {
                    endpoint,
                    resource,
                    captures,
                    fallback_head: true,
                };
            }
        }

        Route::MethodNotAllowed { resource, captures }
    }
}

#[derive(Debug)]
pub(super) struct Config {
    pub(super) fallback_head: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fallback_head: true,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) struct ResourceId(pub(super) usize);

/// A type representing a set of endpoints with the same HTTP path.
pub(super) struct Resource {
    pub(super) id: ResourceId,
    pub(super) uri: Uri,
    pub(super) endpoints: Vec<Endpoint>,
    pub(super) fallback: Option<Arc<dyn Fallback + Send + Sync + 'static>>,
    pub(super) allowed_methods: IndexMap<Method, usize>,
    pub(super) allowed_methods_value: HeaderValue,
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
    fn recognize(&self, method: &Method) -> Option<&Endpoint> {
        self.allowed_methods
            .get(method)
            .map(|&pos| &self.endpoints[pos])
    }

    pub(super) fn update(&mut self) {
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
pub(super) struct Endpoint {
    pub(super) id: usize,
    pub(super) uri: Uri,
    pub(super) methods: IndexSet<Method>,
    pub(super) handler: BoxedHandler,
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
pub(super) enum Route<'a> {
    /// The URI is matched and a route associated with the specified method is found.
    Matched {
        endpoint: &'a Endpoint,
        resource: &'a Resource,
        captures: Option<Captures>,
        fallback_head: bool,
    },

    /// The URI is not matched to any endpoints.
    NotFound,

    /// the URI is matched, but the method is disallowed.
    MethodNotAllowed {
        resource: &'a Resource,
        captures: Option<Captures>,
    },
}
