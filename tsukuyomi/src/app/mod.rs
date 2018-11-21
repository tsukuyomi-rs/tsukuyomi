//! Components for constructing HTTP applications.

pub mod route;
pub mod scope;

mod builder;
mod callback;
mod error;
pub(crate) mod imp;
#[cfg(test)]
mod tests;

pub use {
    self::{
        builder::Builder,
        callback::ErrorHandler,
        error::{Error, Result},
        route::Route,
        scope::Scope,
    },
    crate::{route, scope},
};
use {
    crate::{
        error::Critical,
        handler::Handler,
        input::RequestBody,
        modifier::Modifier,
        output::ResponseBody,
        recognizer::{Captures, Recognizer},
        scoped_map::{ScopeId, ScopedContainer},
        uri::Uri,
    },
    futures::{Async, Poll},
    http::{header::HeaderValue, Method, Request, Response},
    indexmap::{IndexMap, IndexSet},
    std::{fmt, sync::Arc},
    tower_service::{NewService, Service},
};

pub fn app() -> self::builder::Builder<(), ()> {
    self::builder::Builder::default()
}

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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RouteId(pub(crate) ScopeId, pub(crate) usize);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct EndpointId(usize);

/// The global and shared variables used throughout the serving an HTTP application.
struct AppData {
    routes: Vec<RouteData>,
    scopes: Vec<ScopeData>,
    global_scope: ScopeData,

    recognizer: Recognizer,
    endpoints: Vec<EndpointData>,

    states: ScopedContainer,
    on_error: Box<dyn ErrorHandler + Send + Sync + 'static>,
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
        &self.endpoints[id.0].uri
    }

    pub(crate) fn get_state<T>(&self, id: RouteId) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        self.states.get(id.0)
    }

    fn get_scope(&self, id: ScopeId) -> Option<&ScopeData> {
        match id {
            ScopeId::Global => Some(&self.global_scope),
            ScopeId::Local(id) => self.scopes.get(id),
        }
    }

    fn recognize(&self, path: &str, method: &Method) -> Recognize<'_> {
        let (i, captures) = match self.recognizer.recognize(path) {
            Some(result) => result,
            None => return Recognize::NotFound,
        };

        let endpoint = &self.endpoints[i];
        debug_assert_eq!(endpoint.id.0, i);

        if let Some(&pos) = endpoint.route_ids.get(method) {
            let route = &self.routes[pos];
            debug_assert_eq!(route.id.1, pos);
            return Recognize::Matched {
                route,
                endpoint,
                captures,
                fallback_head: false,
            };
        }

        if self.config.fallback_head && *method == Method::HEAD {
            if let Some(&pos) = endpoint.route_ids.get(&Method::GET) {
                let route = &self.routes[pos];
                debug_assert_eq!(route.id.1, pos);
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
    parent: ScopeId,
    prefix: Option<Uri>,
    modifier: Box<dyn Modifier + Send + Sync + 'static>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeData")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("prefix", &self.prefix)
            .finish()
    }
}

struct RouteData {
    id: RouteId,
    uri: Uri,
    methods: IndexSet<Method>,
    handler: Box<dyn Handler + Send + Sync + 'static>,
    modifier_ids: Vec<ScopeId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for RouteData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteData")
            .field("id", &self.id)
            .field("uri", &self.uri)
            .field("methods", &self.methods)
            .field("modifier_ids", &self.modifier_ids)
            .finish()
    }
}

#[derive(Debug)]
struct EndpointData {
    id: EndpointId,
    uri: Uri,
    route_ids: IndexMap<Method, usize>,
    allowed_methods: HeaderValue,
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
    NotFound,

    /// the URI is matched, but
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
