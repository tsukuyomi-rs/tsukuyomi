//! Components for constructing HTTP applications.

pub mod config;
pub mod route;

mod error;
mod recognizer;
mod service;
mod tree;
mod uri;

#[cfg(test)]
mod tests;

pub use self::{
    config::AppConfig,
    error::{Error, Result},
    service::AppService,
};

pub(crate) use self::{recognizer::Captures, uri::CaptureNames};

use {
    self::{
        recognizer::{RecognizeError, Recognizer},
        tree::{Arena, Node, NodeId},
        uri::Uri,
    },
    crate::{
        core::{Never, TryFrom}, //
        handler::BoxedHandler,
        input::body::RequestBody,
        output::ResponseBody,
    },
    http::{header::HeaderValue, HttpTryFrom, Method, Request, Response},
    indexmap::{indexset, IndexSet},
    std::{iter::FromIterator, sync::Arc},
    tower_service::NewService,
};

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    inner: Arc<AppInner>,
}

impl App {
    pub fn configure(config: impl AppConfig<()>) -> Result<Self> {
        Self::with_prefix("/", config)
    }

    pub fn with_prefix(prefix: impl AsRef<str>, config: impl AppConfig<()>) -> Result<Self> {
        self::config::configure(prefix, config)
    }
}

impl NewService for App {
    type Request = Request<RequestBody>;
    type Response = Response<ResponseBody>;
    type Error = Never;
    type Service = AppService;
    type InitError = Never;
    type Future = futures01::future::FutureResult<Self::Service, Self::InitError>;

    fn new_service(&self) -> Self::Future {
        futures01::future::ok(AppService {
            inner: self.inner.clone(),
        })
    }
}

#[derive(Debug)]
struct AppInner {
    recognizer: Recognizer<Resource>,
    scopes: Arena<ScopeData>,
}

impl AppInner {
    fn scope(&self, id: NodeId) -> &Node<ScopeData> {
        &self.scopes[id]
    }

    fn resource(&self, id: ResourceId) -> &Resource {
        self.recognizer.get(id.0).expect("the wrong resource ID")
    }

    /// Infers the scope where the input path belongs from the extracted candidates.
    fn infer_scope<'a>(
        &self,
        path: &str,
        resources: impl IntoIterator<Item = &'a Resource>,
    ) -> &Node<ScopeData> {
        // First, extract a series of common ancestors of candidates.
        let ancestors = {
            let mut ancestors: Option<&[NodeId]> = None;
            for resource in resources {
                let ancestors = ancestors.get_or_insert(&resource.ancestors);
                let n = (*ancestors)
                    .iter()
                    .zip(&resource.ancestors)
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| std::cmp::min(ancestors.len(), resource.ancestors.len()));
                *ancestors = &ancestors[..n];
            }
            ancestors
        };

        // Then, find the oldest ancestor that with the input path as the prefix of URI.
        let node_id = ancestors
            .and_then(|ancestors| {
                ancestors
                    .into_iter()
                    .find(|&&scope| self.scope(scope).data.prefix.as_str().starts_with(path)) //
                    .or_else(|| ancestors.last())
                    .cloned()
            })
            .unwrap_or_else(NodeId::root);

        self.scope(node_id)
    }

    fn find_fallback(&self, start: NodeId) -> Option<&BoxedHandler> {
        let scope = self.scope(start);
        if let Some(ref f) = scope.data.fallback {
            return Some(f);
        }
        scope
            .ancestors()
            .into_iter()
            .rev()
            .filter_map(|&id| self.scope(id).data.fallback.as_ref())
            .next()
    }

    fn route(
        &self,
        path: &str,
        captures: &mut Option<Captures>,
    ) -> std::result::Result<&Resource, &Node<ScopeData>> {
        match self.recognizer.recognize(path, captures) {
            Ok(resource) => Ok(resource),
            Err(RecognizeError::NotMatched) => Err(self.scope(NodeId::root())),
            Err(RecognizeError::PartiallyMatched(candidates)) => Err(self.infer_scope(
                path,
                candidates.iter().filter_map(|i| self.recognizer.get(i)),
            )),
        }
    }
}

#[derive(Debug)]
struct ScopeData {
    prefix: Uri,
    fallback: Option<BoxedHandler>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct ResourceId(usize);

/// A type representing a set of endpoints with the same HTTP path.
#[derive(Debug)]
pub struct Resource {
    id: ResourceId,
    scope: NodeId,
    ancestors: Vec<NodeId>,
    uri: Uri,
    allowed_methods: Option<AllowedMethods>,
    handler: BoxedHandler,
}

impl Resource {
    #[doc(hidden)]
    pub fn allowed_methods(&self) -> Option<&AllowedMethods> {
        self.allowed_methods.as_ref()
    }
}

/// A set of request methods that a route accepts.
#[derive(Debug, Clone)]
pub struct AllowedMethods(Arc<IndexSet<Method>>);

impl From<Method> for AllowedMethods {
    fn from(method: Method) -> Self {
        AllowedMethods(Arc::new(indexset! { method }))
    }
}

impl<M> FromIterator<M> for AllowedMethods
where
    M: Into<Method>,
{
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = M>,
    {
        AllowedMethods(Arc::new(iter.into_iter().map(Into::into).collect()))
    }
}

impl AllowedMethods {
    pub fn contains(&self, method: &Method) -> bool {
        self.0.contains(method)
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a Method> + 'a {
        self.0.iter()
    }

    pub fn render_with_options(&self) -> HeaderValue {
        let mut bytes = bytes::BytesMut::new();
        for (i, method) in self.iter().enumerate() {
            if i > 0 {
                bytes.extend_from_slice(b", ");
            }
            bytes.extend_from_slice(method.as_str().as_bytes());
        }
        if !self.0.contains(&Method::OPTIONS) {
            if !self.0.is_empty() {
                bytes.extend_from_slice(b", ");
            }
            bytes.extend_from_slice(b"OPTIONS");
        }

        unsafe { HeaderValue::from_shared_unchecked(bytes.freeze()) }
    }
}

impl TryFrom<Self> for AllowedMethods {
    type Error = Never;

    #[inline]
    fn try_from(methods: Self) -> std::result::Result<Self, Self::Error> {
        Ok(methods)
    }
}

impl TryFrom<Method> for AllowedMethods {
    type Error = Never;

    #[inline]
    fn try_from(method: Method) -> std::result::Result<Self, Self::Error> {
        Ok(AllowedMethods::from(method))
    }
}

impl<M> TryFrom<Vec<M>> for AllowedMethods
where
    Method: HttpTryFrom<M>,
{
    type Error = http::Error;

    #[inline]
    fn try_from(methods: Vec<M>) -> std::result::Result<Self, Self::Error> {
        let methods: Vec<_> = methods
            .into_iter()
            .map(Method::try_from)
            .collect::<std::result::Result<_, _>>()
            .map_err(Into::into)?;
        Ok(AllowedMethods::from_iter(methods))
    }
}

impl<'a> TryFrom<&'a str> for AllowedMethods {
    type Error = failure::Error;

    #[inline]
    fn try_from(methods: &'a str) -> std::result::Result<Self, Self::Error> {
        let methods: Vec<_> = methods
            .split(',')
            .map(|s| Method::try_from(s.trim()).map_err(Into::into))
            .collect::<http::Result<_>>()?;
        Ok(AllowedMethods::from_iter(methods))
    }
}
