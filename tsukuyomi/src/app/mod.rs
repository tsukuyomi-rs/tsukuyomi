//! Components for constructing HTTP applications.

pub mod fallback;
pub mod route;
pub mod scope;

mod builder;
mod error;
pub(crate) mod imp;
#[cfg(test)]
mod tests;

#[allow(deprecated)]
pub use {self::route::Route, crate::scope};
pub use {
    self::{
        builder::Builder,
        error::{Error, Result},
        scope::Scope,
    },
    crate::route,
};
use {
    crate::{
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

pub fn app() -> self::builder::Builder<()> {
    self::builder::Builder::default()
}

#[deprecated(since = "0.4.2", note = "use `scope::mount` instead")]
#[allow(deprecated)]
pub fn scope() -> self::scope::Builder<()> {
    self::scope::Builder::<()>::default()
}

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
    routes: Vec<RouteData>,
    scopes: Vec<ScopeData>,
    global_scope: ScopeData,

    recognizer: Recognizer,
    endpoints: IndexMap<Uri, EndpointData>,

    states: ScopedContainer,
    config: Config,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppData")
            .field("routes", &self.routes)
            .field("scopes", &self.scopes)
            .field("global_scope", &self.global_scope)
            .field("recognizer", &self.recognizer)
            .field("endpoints", &self.endpoints)
            .field("states", &self.states)
            .field("config", &self.config)
            .finish()
    }
}

impl AppData {
    fn uri(&self, id: EndpointId) -> &Uri {
        self.endpoints
            .get_index(id.1)
            .map(|(uri, _endpoint)| uri)
            .expect("the wrong endpoint ID")
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

    /// Infers the scope ID where the input path belongs from the extract candidates of endpoint indices.
    fn infer_scope_id(&self, path: &str, candidates: &Candidates) -> Option<ScopeId> {
        // First, extract a series of common ancestors of candidates.
        let ancestors = {
            let mut ancestors: Option<&[ScopeId]> = None;
            for (_, endpoint) in candidates
                .iter()
                .filter_map(|i| self.endpoints.get_index(i))
            {
                let ancestors = ancestors.get_or_insert(&endpoint.parents);
                let n = (*ancestors)
                    .iter()
                    .zip(&endpoint.parents)
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| std::cmp::min(ancestors.len(), endpoint.parents.len()));
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

        let (_, endpoint) = &self
            .endpoints
            .get_index(i)
            .expect("the wrong index was registered in recognizer");
        debug_assert_eq!(endpoint.id.1, i);

        if let Some(&id) = endpoint.route_ids.get(method) {
            let route = &self.routes[id.1];
            debug_assert_eq!(route.id, id);
            return Recognize::Matched {
                route,
                endpoint,
                captures,
                fallback_head: false,
            };
        }

        if self.config.fallback_head && *method == Method::HEAD {
            if let Some(&id) = endpoint.route_ids.get(&Method::GET) {
                let route = &self.routes[id.1];
                debug_assert_eq!(route.id, id);
                return Recognize::Matched {
                    route,
                    endpoint,
                    captures,
                    fallback_head: true,
                };
            }
        }

        Recognize::MethodNotAllowed { endpoint, captures }
    }
}

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
struct EndpointId(ScopeId, usize);

#[derive(Debug)]
struct EndpointData {
    id: EndpointId,
    uri: Uri,
    route_ids: IndexMap<Method, RouteId>,
    allowed_methods_value: HeaderValue,
    parents: Vec<ScopeId>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct RouteId(EndpointId, usize);

struct RouteData {
    id: RouteId,
    uri: Uri,
    methods: IndexSet<Method>,
    handler: Box<dyn Handler + Send + Sync + 'static>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for RouteData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteData")
            .field("uri", &self.uri)
            .field("methods", &self.methods)
            .finish()
    }
}

#[derive(Debug)]
enum Recognize<'a> {
    /// The URI is matched and a route associated with the specified method is found.
    Matched {
        route: &'a RouteData,
        endpoint: &'a EndpointData,
        captures: Option<Captures>,
        fallback_head: bool,
    },

    /// The URI is not matched to any endpoints.
    NotFound(ScopeId),

    /// the URI is matched, but the method is disallowed.
    MethodNotAllowed {
        endpoint: &'a EndpointData,
        captures: Option<Captures>,
    },
}

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    data: Arc<AppData>,
}

impl App {
    #[doc(hidden)]
    #[deprecated(note = "use `tsukuyomi::app` instead.")]
    pub fn builder() -> Builder {
        Builder::default()
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
