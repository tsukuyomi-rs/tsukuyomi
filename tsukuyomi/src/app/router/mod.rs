mod recognizer;

use {
    self::recognizer::RecognizeError,
    super::{fallback::Fallback, Uri},
    crate::handler::BoxedHandler,
    bytes::BytesMut,
    http::{header::HeaderValue, Method},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
};

pub(super) use self::recognizer::{Captures, Recognizer};

pub(super) struct Router {
    pub(super) recognizer: Recognizer<Resource>,
    pub(super) global_fallback: Option<Arc<Box<dyn Fallback + Send + Sync + 'static>>>,
}

impl fmt::Debug for Router {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Router")
            .field("recognizer", &self.recognizer)
            .finish()
    }
}

impl Router {
    pub(super) fn resource(&self, id: ResourceId) -> &Resource {
        self.recognizer.get(id.0).expect("the wrong resource ID")
    }

    pub(super) fn route(&self, path: &str, method: &Method) -> Route<'_> {
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) struct ResourceId(pub(super) usize);

/// A type representing a set of endpoints with the same HTTP path.
pub struct Resource {
    pub(super) id: ResourceId,
    pub(super) uri: Uri,
    pub(super) endpoints: Vec<Endpoint>,
    pub(super) fallback: Option<Arc<Box<dyn Fallback + Send + Sync + 'static>>>,
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
    pub fn allowed_methods<'a>(&'a self) -> impl Iterator<Item = &'a Method> + 'a {
        self.allowed_methods.keys()
    }

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
pub struct Endpoint {
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
