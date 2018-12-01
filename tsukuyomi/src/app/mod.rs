//! Components for constructing HTTP applications.

pub mod fallback;
pub mod route;
pub mod scope;

/// A *prelude* for using the primitive `Scope`s.
pub mod directives {
    #[doc(no_inline)]
    pub use {
        super::{
            scope::{mount, route},
            App,
        },
        crate::route2 as route,
    };

    use {
        super::{
            fallback::{Fallback, FallbackInstance},
            scope::Scope,
        },
        crate::{common::Never, modifier::Modifier},
    };

    /// Creates a `Scope` that registers the specified state to be shared into the scope.
    #[allow(deprecated)]
    pub fn state<T>(state: T) -> impl Scope<Error = Never>
    where
        T: Send + Sync + 'static,
    {
        super::scope::raw(move |cx| {
            cx.set_state(state);
            Ok(())
        })
    }

    /// Creates a `Scope` that registers the specified `Modifier` into the scope.
    #[allow(deprecated)]
    pub fn modifier<M>(modifier: M) -> impl Scope<Error = Never>
    where
        M: Modifier + Send + Sync + 'static,
    {
        super::scope::raw(move |cx| {
            cx.add_modifier(modifier);
            Ok(())
        })
    }

    /// Creates a `Scope` that registers the specified `Fallback` into the scope.
    pub fn fallback<F>(fallback: F) -> impl Scope<Error = Never>
    where
        F: Fallback + Send + Sync + 'static,
    {
        state(FallbackInstance::from(fallback))
    }
}

mod builder;
mod error;
pub(crate) mod imp;
#[cfg(test)]
mod tests;

#[doc(hidden)]
#[allow(deprecated)]
pub use {
    self::route::Route,
    crate::{route, scope},
};

pub use self::{
    builder::Builder,
    error::{Error, Result},
    scope::Scope,
};
use {
    crate::{
        common::TryFrom,
        error::Critical,
        handler::Handler,
        input::RequestBody,
        modifier::Modifier,
        output::ResponseBody,
        recognizer::{Candidates, Captures, RecognizeError, Recognizer},
        scoped_map::{ScopeId, ScopedContainer},
        uri::Uri,
    },
    futures::{Async, Poll},
    http::{header::HeaderValue, Method, Request, Response},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
    tower_service::{NewService, Service},
};

#[doc(hidden)]
#[deprecated(since = "0.4.2", note = "use `App::builder` instead")]
pub fn app() -> self::builder::Builder<()> {
    self::builder::Builder::default()
}

#[doc(hidden)]
#[deprecated(since = "0.4.2", note = "use `scope::mount` instead")]
#[allow(deprecated)]
pub fn scope() -> self::scope::Builder<()> {
    self::scope::Builder::<()>::default()
}

#[doc(hidden)]
#[deprecated(since = "0.4.2", note = "use `scope::route` instead")]
#[allow(deprecated)]
pub fn route() -> self::route::Builder<()> {
    self::route::Builder::<()>::default()
}

#[derive(Debug)]
struct Config {
    fallback_head: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fallback_head: true,
        }
    }
}

/// The global and shared variables used throughout the serving an HTTP application.
struct AppData {
    endpoints: Vec<Endpoint>,
    scopes: Vec<ScopeData>,
    global_scope: ScopeData,

    recognizer: Recognizer,
    resources: IndexMap<Uri, Resource>,

    states: ScopedContainer,
    config: Config,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppData")
            .field("endpoints", &self.endpoints)
            .field("scopes", &self.scopes)
            .field("global_scope", &self.global_scope)
            .field("recognizer", &self.recognizer)
            .field("resources", &self.resources)
            .field("states", &self.states)
            .field("config", &self.config)
            .finish()
    }
}

impl AppData {
    fn uri(&self, id: ResourceId) -> &Uri {
        self.resources
            .get_index(id.1)
            .map(|(uri, _endpoint)| uri)
            .expect("the wrong resource ID")
    }

    fn get_state<T>(&self, id: ScopeId) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.states.get(id)
    }

    fn scope(&self, id: ScopeId) -> &ScopeData {
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
            for (_, resource) in candidates
                .iter()
                .filter_map(|i| self.resources.get_index(i))
            {
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
            .find(|&&scope| {
                self.scope(scope)
                    .uri
                    .as_ref()
                    .map_or(false, |uri| uri.as_str().starts_with(path))
            }) //
            .or_else(|| ancestors.last())
            .cloned()
    }

    fn recognize(&self, path: &str, method: &Method) -> Recognize<'_> {
        let mut captures = None;
        let i = match self.recognizer.recognize(path, &mut captures) {
            Ok(i) => i,
            Err(RecognizeError::NotMatched) => return Recognize::NotFound(ScopeId::Global),
            Err(RecognizeError::PartiallyMatched(candidates)) => {
                return Recognize::NotFound(
                    self.infer_scope_id(path, candidates)
                        .unwrap_or(ScopeId::Global),
                )
            }
        };

        let (_, resource) = &self
            .resources
            .get_index(i)
            .expect("the wrong index was registered in recognizer");
        debug_assert_eq!(resource.id.1, i);

        if let Some(&id) = resource.route_ids.get(method) {
            let endpoint = &self.endpoints[id.1];
            debug_assert_eq!(endpoint.id, id);
            return Recognize::Matched {
                endpoint,
                resource,
                captures,
                fallback_head: false,
            };
        }

        if self.config.fallback_head && *method == Method::HEAD {
            if let Some(&id) = resource.route_ids.get(&Method::GET) {
                let endpoint = &self.endpoints[id.1];
                debug_assert_eq!(endpoint.id, id);
                return Recognize::Matched {
                    endpoint,
                    resource,
                    captures,
                    fallback_head: true,
                };
            }
        }

        Recognize::MethodNotAllowed { resource, captures }
    }
}

/// A type representing a set of data associated with the certain scope.
struct ScopeData {
    id: ScopeId,
    parents: Vec<ScopeId>,
    prefix: Option<Uri>,
    uri: Option<Uri>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeData")
            .field("id", &self.id)
            .field("parents", &self.parents)
            .field("prefix", &self.prefix)
            .field("uri", &self.uri)
            .finish()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct ResourceId(ScopeId, usize);

/// A type representing a set of endpoints with the same HTTP path.
#[derive(Debug)]
struct Resource {
    id: ResourceId,
    uri: Uri,
    route_ids: IndexMap<Method, EndpointId>,
    allowed_methods_value: HeaderValue,
    parents: Vec<ScopeId>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct EndpointId(ResourceId, usize);

/// A struct representing a set of data associated with an endpoint.
struct Endpoint {
    id: EndpointId,
    uri: Uri,
    methods: IndexSet<Method>,
    handler: Box<dyn Handler + Send + Sync + 'static>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("uri", &self.uri)
            .field("methods", &self.methods)
            .finish()
    }
}

#[derive(Debug)]
enum Recognize<'a> {
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

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    data: Arc<AppData>,
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
    type Future = futures::future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        futures::future::ok(AppService {
            data: self.data.clone(),
        })
    }
}

/// The instance of `Service` generated by `App`.
#[derive(Debug)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
pub struct AppService {
    data: Arc<AppData>,
}

impl Service for AppService {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = Critical;
    type Future = self::imp::AppFuture;

    #[inline]
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    #[inline]
    fn call(&mut self, request: Self::Request) -> Self::Future {
        self::imp::AppFuture::new(request, self.data.clone())
    }
}
