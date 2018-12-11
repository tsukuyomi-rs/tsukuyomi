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
        core::Never, //
        handler::BoxedHandler,
        input::body::RequestBody,
        output::ResponseBody,
    },
    http::{Request, Response},
    std::sync::Arc,
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

    fn find_fallback(&self, start: NodeId) -> Option<&(dyn BoxedHandler + Send + Sync + 'static)> {
        let scope = self.scope(start);
        if let Some(ref f) = scope.data.fallback {
            return Some(&**f);
        }
        scope
            .ancestors()
            .into_iter()
            .rev()
            .filter_map(|&id| self.scope(id).data.fallback.as_ref().map(|f| &**f))
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
    fallback: Option<Box<dyn BoxedHandler + Send + Sync + 'static>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct ResourceId(usize);

/// A type representing a set of endpoints with the same HTTP path.
#[derive(Debug)]
struct Resource {
    id: ResourceId,
    scope: NodeId,
    ancestors: Vec<NodeId>,
    uri: Uri,
    handler: Box<dyn BoxedHandler + Send + Sync + 'static>,
}
