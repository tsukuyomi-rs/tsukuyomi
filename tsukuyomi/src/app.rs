//! Components for constructing HTTP applications.

mod config;
mod recognizer;
mod scope;
mod service;

#[cfg(test)]
mod tests;

pub(crate) use self::recognizer::Captures;
pub use self::{
    config::{Config, Error, Result, Scope},
    service::AppService,
};

use {
    self::{
        recognizer::{RecognizeError, Recognizer},
        scope::{Scope as ScopeNode, ScopeId, Scopes},
    },
    crate::{
        handler::{Handle, Handler}, //
        input::Input,
        output::{IntoResponse, Responder, ResponseBody},
        uri::Uri,
    },
    futures01::{Async, Future, Poll},
    http::Response,
    std::{fmt, sync::Arc},
};

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App {
    inner: Arc<AppInner>,
}

impl App {
    /// Creates a new `App` from the provided configuration.
    pub fn create(config: impl Config<()>) -> self::config::Result<Self> {
        self::config::configure(config)
    }
}

#[derive(Debug)]
struct AppInner {
    recognizer: Recognizer<Endpoint>,
    scopes: Scopes<ScopeData>,
}

impl AppInner {
    fn scope(&self, id: ScopeId) -> &ScopeNode<ScopeData> {
        &self.scopes[id]
    }

    fn endpoint(&self, id: EndpointId) -> &Endpoint {
        self.recognizer.get(id.0).expect("the wrong resource ID")
    }

    /// Infers the scope where the input path belongs from the extracted candidates.
    fn infer_scope<'a>(
        &self,
        path: &str,
        endpoints: impl IntoIterator<Item = &'a Endpoint>,
    ) -> &ScopeNode<ScopeData> {
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
                    .into_iter()
                    .find(|&&scope| self.scope(scope).data.prefix.as_str().starts_with(path)) //
                    .or_else(|| ancestors.last())
                    .cloned()
            })
            .unwrap_or_else(ScopeId::root);

        self.scope(node_id)
    }

    fn find_default_handler(&self, start: ScopeId) -> Option<&dyn BoxedHandler> {
        let scope = self.scope(start);
        if let Some(ref f) = scope.data.default_handler {
            return Some(&**f);
        }
        scope
            .ancestors()
            .into_iter()
            .rev()
            .filter_map(|&id| self.scope(id).data.default_handler.as_ref().map(|f| &**f))
            .next()
    }

    fn find_endpoint(
        &self,
        path: &str,
        captures: &mut Option<Captures>,
    ) -> std::result::Result<&Endpoint, &ScopeNode<ScopeData>> {
        match self.recognizer.recognize(path, captures) {
            Ok(endpoint) => Ok(endpoint),
            Err(RecognizeError::NotMatched) => Err(self.scope(ScopeId::root())),
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
    default_handler: Option<Box<dyn BoxedHandler>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct EndpointId(usize);

/// A type representing a set of endpoints with the same HTTP path.
#[derive(Debug)]
struct Endpoint {
    id: EndpointId,
    scope: ScopeId,
    ancestors: Vec<ScopeId>,
    uri: Uri,
    handler: Box<dyn BoxedHandler>,
}

type BoxedHandle =
    dyn FnMut(&mut Input<'_>) -> Poll<Response<ResponseBody>, crate::error::Error> + Send + 'static;

trait BoxedHandler: Send + Sync + 'static {
    fn call(&self) -> Box<BoxedHandle>;
}

impl fmt::Debug for dyn BoxedHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoxedHandler").finish()
    }
}

impl<H> BoxedHandler for H
where
    H: Handler + Send + Sync + 'static,
    H::Output: Responder,
    <H::Output as Responder>::Future: Send + 'static,
    H::Handle: Send + 'static,
{
    fn call(&self) -> Box<BoxedHandle> {
        enum State<A, B> {
            First(A),
            Second(B),
        }

        let mut state: State<H::Handle, <H::Output as Responder>::Future> =
            State::First(self.handle());

        Box::new(move |input| loop {
            state = match state {
                State::First(ref mut handle) => {
                    let x = futures01::try_ready!(handle.poll_ready(input).map_err(Into::into));
                    State::Second(x.respond(input))
                }
                State::Second(ref mut respond) => {
                    return Ok(Async::Ready(
                        futures01::try_ready!(respond.poll().map_err(Into::into))
                            .into_response(input.request)
                            .map_err(Into::into)?
                            .map(Into::into),
                    ));
                }
            };
        })
    }
}
