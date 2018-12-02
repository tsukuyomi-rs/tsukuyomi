mod recognizer;

use {
    self::recognizer::{Candidates, RecognizeError},
    super::{scoped_map::ScopeId, Uri},
    bytes::BytesMut,
    crate::{handler::Handler, modifier::Modifier},
    http::{header::HeaderValue, Method},
    indexmap::{IndexMap, IndexSet},
    std::fmt,
};

pub(super) use self::recognizer::{Captures, Recognizer};

#[derive(Debug)]
pub(super) struct Router {
    pub(super) recognizer: Recognizer<Resource>,
    pub(super) scopes: Vec<Scope>,
    pub(super) global_scope: Scope,
    pub(super) config: Config,
}

impl Router {
    pub(super) fn resource(&self, id: ResourceId) -> &Resource {
        self.recognizer.get(id.1).expect("the wrong resource ID")
    }

    pub(super) fn scope(&self, id: ScopeId) -> &Scope {
        match id {
            ScopeId::Global => &self.global_scope,
            ScopeId::Local(id) => &self.scopes[id],
        }
    }

    /// Infers the scope ID where the input path belongs from the extract candidates of resource indices.
    fn infer_scope_id(&self, path: &str, candidates: &Candidates) -> Option<ScopeId> {
        // First, extract a series of common ancestors of candidates.
        let ancestors = {
            let mut ancestors: Option<&[ScopeId]> = None;
            for resource in candidates.iter().filter_map(|i| self.recognizer.get(i)) {
                let ancestors = ancestors.get_or_insert(&resource.parents);
                let n = (*ancestors)
                    .iter()
                    .zip(&resource.parents)
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| std::cmp::min(ancestors.len(), resource.parents.len()));
                *ancestors = &ancestors[..n];
            }
            ancestors?
        };

        // Then, find the oldest ancestor that with the input path as the prefix of URI.
        ancestors
            .into_iter()
            .find(|&&scope| self.scope(scope).prefix.as_str().starts_with(path)) //
            .or_else(|| ancestors.last())
            .cloned()
    }

    pub(super) fn route(&self, path: &str, method: &Method) -> Route<'_> {
        let mut captures = None;
        let resource = match self.recognizer.recognize(path, &mut captures) {
            Ok(resource) => resource,
            Err(RecognizeError::NotMatched) => return Route::NotFound(ScopeId::Global),
            Err(RecognizeError::PartiallyMatched(candidates)) => {
                return Route::NotFound(
                    self.infer_scope_id(path, candidates)
                        .unwrap_or(ScopeId::Global),
                )
            }
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
pub(super) struct ResourceId(pub(super) ScopeId, pub(super) usize);

/// A type representing a set of endpoints with the same HTTP path.
#[derive(Debug)]
pub(super) struct Resource {
    pub(super) id: ResourceId,
    pub(super) uri: Uri,
    pub(super) endpoints: Vec<Endpoint>,
    pub(super) allowed_methods: IndexMap<Method, usize>,
    pub(super) allowed_methods_value: HeaderValue,
    pub(super) parents: Vec<ScopeId>,
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
    pub(super) handler: Box<dyn Handler + Send + Sync + 'static>,
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

/// A type representing a set of data associated with the certain scope.
pub(super) struct Scope {
    pub(super) id: ScopeId,
    pub(super) prefix: Uri,
    pub(super) parents: Vec<ScopeId>,
    pub(super) modifier: Box<dyn Modifier + Send + Sync + 'static>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope")
            .field("id", &self.id)
            .field("prefix", &self.prefix)
            .field("parents", &self.parents)
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
    NotFound(ScopeId),

    /// the URI is matched, but the method is disallowed.
    MethodNotAllowed {
        resource: &'a Resource,
        captures: Option<Captures>,
    },
}
