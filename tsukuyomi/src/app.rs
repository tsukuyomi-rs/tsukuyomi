//! Components for constructing HTTP applications.

pub mod concurrency;
pub mod config;
pub mod path;

mod recognizer;
mod scope;
mod service;

#[cfg(test)]
mod tests;

pub(crate) use self::recognizer::Captures;
pub use self::{
    config::{Error, Result},
    service::{AppBody, AppService},
};

use {
    self::{
        concurrency::{Concurrency, DefaultConcurrency},
        recognizer::{RecognizeError, Recognizer},
        scope::{Scope, ScopeId, Scopes},
    },
    crate::{input::localmap::local_key, uri::Uri},
    std::{fmt, sync::Arc},
};

local_key! {
    pub const REMOTE_ADDR: std::net::SocketAddr;
}

/// The main type representing an HTTP application.
#[derive(Debug)]
pub struct App<C: Concurrency = DefaultConcurrency> {
    inner: Arc<AppInner<C>>,
}

impl<C: Concurrency> Clone for App<C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<C: Concurrency> App<C> {
    /// Creates a new instance of `AppService` associated with this `App`.
    pub fn new_service(&self) -> AppService<C> {
        AppService::new(self.inner.clone())
    }
}

#[derive(Debug)]
struct AppInner<C: Concurrency> {
    recognizer: Recognizer<Arc<Endpoint<C>>>,
    scopes: Scopes<ScopeData<C>>,
}

impl<C: Concurrency> AppInner<C> {
    fn scope(&self, id: ScopeId) -> &Scope<ScopeData<C>> {
        &self.scopes[id]
    }

    /// Infers the scope where the input path belongs from the extracted candidates.
    fn infer_scope<'a>(
        &self,
        path: &str,
        endpoints: impl IntoIterator<Item = &'a Endpoint<C>>,
    ) -> &Scope<ScopeData<C>> {
        // First, extract a series of common ancestors of candidates.
        let ancestors = {
            let mut ancestors: Option<&[ScopeId]> = None;
            for endpoint in endpoints {
                let ancestors = ancestors.get_or_insert(&endpoint.ancestors);
                let n = (*ancestors)
                    .iter()
                    .zip(&endpoint.ancestors)
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| std::cmp::min(ancestors.len(), endpoint.ancestors.len()));
                *ancestors = &ancestors[..n];
            }
            ancestors
        };

        // Then, find the oldest ancestor that with the input path as the prefix of URI.
        let node_id = ancestors
            .and_then(|ancestors| {
                ancestors
                    .iter()
                    .find(|&&scope| self.scope(scope).data.prefix.as_str().starts_with(path)) //
                    .or_else(|| ancestors.last())
                    .cloned()
            })
            .unwrap_or_else(ScopeId::root);

        self.scope(node_id)
    }

    fn find_default_handler(&self, start: ScopeId) -> Option<&C::Handler> {
        let scope = self.scope(start);
        if let Some(ref f) = scope.data.default_handler {
            return Some(f);
        }
        scope
            .ancestors()
            .iter()
            .rev()
            .filter_map(|&id| self.scope(id).data.default_handler.as_ref())
            .next()
    }

    fn find_endpoint(
        &self,
        path: &str,
        captures: &mut Option<Captures>,
    ) -> std::result::Result<&Arc<Endpoint<C>>, &Scope<ScopeData<C>>> {
        match self.recognizer.recognize(path, captures) {
            Ok(endpoint) => Ok(endpoint),
            Err(RecognizeError::NotMatched) => Err(self.scope(ScopeId::root())),
            Err(RecognizeError::PartiallyMatched(candidates)) => Err(self.infer_scope(
                path,
                candidates
                    .iter()
                    .filter_map(|i| self.recognizer.get(i).map(|e| &**e)),
            )),
        }
    }
}

struct ScopeData<C: Concurrency> {
    prefix: Uri,
    default_handler: Option<C::Handler>,
}

impl<C: Concurrency> fmt::Debug for ScopeData<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeData")
            .field("prefix", &self.prefix)
            .field(
                "default_handler",
                &self.default_handler.as_ref().map(|_| "<default handler>"),
            )
            .finish()
    }
}

/// A type representing a set of endpoints with the same HTTP path.
struct Endpoint<C: Concurrency> {
    scope: ScopeId,
    ancestors: Vec<ScopeId>,
    uri: Uri,
    handler: C::Handler,
}

impl<C: Concurrency> fmt::Debug for Endpoint<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("scope", &self.scope)
            .field("ancestors", &self.ancestors)
            .field("uri", &self.uri)
            .finish()
    }
}
