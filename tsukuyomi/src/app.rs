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
    crate::{input::Input, output::ResponseBody, uri::Uri},
    futures01::Poll,
    http::Response,
    std::{fmt, sync::Arc},
};

/// The main type which represents an HTTP application.
#[derive(Debug, Clone)]
pub struct App<T: AppData = ThreadSafe> {
    inner: Arc<AppInner<T>>,
}

impl App<ThreadSafe> {
    /// Creates a new `App` from the provided configuration.
    pub fn create(config: impl Config<(), ThreadSafe>) -> self::config::Result<Self> {
        self::config::configure(config)
    }
}

impl App<CurrentThread> {
    /// Creates a new `App` from the provided configuration.
    pub fn create_local(config: impl Config<(), CurrentThread>) -> self::config::Result<Self> {
        self::config::configure(config)
    }
}

#[derive(Debug)]
struct AppInner<T: AppData> {
    recognizer: Recognizer<Endpoint<T>>,
    scopes: Scopes<ScopeData<T>>,
}

impl<T: AppData> AppInner<T> {
    fn scope(&self, id: ScopeId) -> &ScopeNode<ScopeData<T>> {
        &self.scopes[id]
    }

    fn endpoint(&self, id: EndpointId) -> &Endpoint<T> {
        self.recognizer.get(id.0).expect("the wrong resource ID")
    }

    /// Infers the scope where the input path belongs from the extracted candidates.
    fn infer_scope<'a>(
        &self,
        path: &str,
        endpoints: impl IntoIterator<Item = &'a Endpoint<T>>,
    ) -> &ScopeNode<ScopeData<T>> {
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

    fn find_default_handler(&self, start: ScopeId) -> Option<&T::Handler> {
        let scope = self.scope(start);
        if let Some(ref f) = scope.data.default_handler {
            return Some(f);
        }
        scope
            .ancestors()
            .into_iter()
            .rev()
            .filter_map(|&id| self.scope(id).data.default_handler.as_ref())
            .next()
    }

    fn find_endpoint(
        &self,
        path: &str,
        captures: &mut Option<Captures>,
    ) -> std::result::Result<&Endpoint<T>, &ScopeNode<ScopeData<T>>> {
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

struct ScopeData<T: AppData> {
    prefix: Uri,
    default_handler: Option<T::Handler>,
}

impl<T: AppData> fmt::Debug for ScopeData<T> {
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct EndpointId(usize);

/// A type representing a set of endpoints with the same HTTP path.
struct Endpoint<T: AppData> {
    id: EndpointId,
    scope: ScopeId,
    ancestors: Vec<ScopeId>,
    uri: Uri,
    handler: T::Handler,
}

impl<T: AppData> fmt::Debug for Endpoint<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("id", &self.id)
            .field("scope", &self.scope)
            .field("ancestors", &self.ancestors)
            .field("uri", &self.uri)
            .finish()
    }
}

// ==== AppData ====

pub trait AppData: Default + Send + Sync + 'static {
    type Handler;
    type Handle;

    fn handle(handler: &Self::Handler) -> Self::Handle;
    fn poll_ready(
        handle: &mut Self::Handle,
        input: &mut Input<'_>,
    ) -> Poll<Response<ResponseBody>, crate::error::Error>;
}

#[derive(Debug, Default)]
pub struct ThreadSafe(());

mod thread_safe {
    use {
        crate::{
            error::Error,
            handler::{Handle, Handler},
            input::Input,
            output::{IntoResponse, Responder, ResponseBody},
        },
        futures01::{Async, Future, Poll},
        http::Response,
        std::fmt,
    };

    impl super::AppData for super::ThreadSafe {
        type Handler = BoxedHandler;
        type Handle = Box<BoxedHandle>;

        fn handle(handler: &Self::Handler) -> Self::Handle {
            (handler.0)()
        }

        fn poll_ready(
            handle: &mut Self::Handle,
            input: &mut Input<'_>,
        ) -> Poll<Response<ResponseBody>, Error> {
            (handle)(input)
        }
    }

    type BoxedHandle =
        dyn FnMut(&mut Input<'_>) -> Poll<Response<ResponseBody>, crate::error::Error>
            + Send
            + 'static;

    pub struct BoxedHandler(Box<dyn Fn() -> Box<BoxedHandle> + Send + Sync + 'static>);

    impl fmt::Debug for BoxedHandler {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("BoxedHandler").finish()
        }
    }

    impl<H> From<H> for BoxedHandler
    where
        H: Handler + Send + Sync + 'static,
        H::Output: Responder,
        <H::Output as Responder>::Future: Send + 'static,
        H::Handle: Send + 'static,
    {
        fn from(handler: H) -> Self {
            BoxedHandler(Box::new(move || {
                enum State<A, B> {
                    First(A),
                    Second(B),
                }

                let mut state: State<H::Handle, <H::Output as Responder>::Future> =
                    State::First(handler.handle());

                Box::new(move |input| loop {
                    state = match state {
                        State::First(ref mut handle) => {
                            let x =
                                futures01::try_ready!(handle.poll_ready(input).map_err(Into::into));
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
            }))
        }
    }
}

#[derive(Debug, Default)]
pub struct CurrentThread(());

mod current_thread {
    use {
        crate::{
            error::Error,
            handler::{Handle, Handler},
            input::Input,
            output::{IntoResponse, Responder, ResponseBody},
        },
        futures01::{Async, Future, Poll},
        http::Response,
        std::fmt,
    };

    impl super::AppData for super::CurrentThread {
        type Handler = BoxedHandler;
        type Handle = Box<BoxedHandle>;

        fn handle(handler: &Self::Handler) -> Self::Handle {
            (handler.0)()
        }

        fn poll_ready(
            handle: &mut Self::Handle,
            input: &mut Input<'_>,
        ) -> Poll<Response<ResponseBody>, Error> {
            (handle)(input)
        }
    }

    type BoxedHandle =
        dyn FnMut(&mut Input<'_>) -> Poll<Response<ResponseBody>, crate::error::Error> + 'static;

    pub struct BoxedHandler(Box<dyn Fn() -> Box<BoxedHandle> + 'static>);

    impl fmt::Debug for BoxedHandler {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("BoxedHandler").finish()
        }
    }

    impl<H> From<H> for BoxedHandler
    where
        H: Handler + 'static,
        H::Output: Responder,
        <H::Output as Responder>::Future: 'static,
        H::Handle: 'static,
    {
        fn from(handler: H) -> Self {
            BoxedHandler(Box::new(move || {
                enum State<A, B> {
                    First(A),
                    Second(B),
                }

                let mut state: State<H::Handle, <H::Output as Responder>::Future> =
                    State::First(handler.handle());

                Box::new(move |input| loop {
                    state = match state {
                        State::First(ref mut handle) => {
                            let x =
                                futures01::try_ready!(handle.poll_ready(input).map_err(Into::into));
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
            }))
        }
    }
}
