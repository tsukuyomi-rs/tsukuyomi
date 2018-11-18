use std::fmt;
use std::sync::Arc;

use bytes::BytesMut;
use http::header::HeaderValue;
use http::{Method, Response};
use indexmap::{IndexMap, IndexSet};

use crate::error::Critical;
use crate::output::{Output, ResponseBody};
use crate::recognizer::Recognizer;
use crate::scoped_map::{Builder as ScopedContainerBuilder, ScopeId};
use crate::uri::Uri;

use super::callback::Callback;
use super::error::{Error, Result};
use super::route::{Context as RouteContext, Handler, Route};
use super::scope::{Context as ScopeContext, Modifier, Scope};
use super::{App, AppData, Config, EndpointData, ModifierId, RouteData, RouteId, ScopeData};

/// A builder object for constructing an instance of `App`.
#[derive(Debug, Default)]
pub struct Builder<S: Scope = (), C: Callback = ()> {
    scope: super::scope::Builder<S>,
    callback: C,
    config: Config,
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl<S, C> Builder<S, C>
where
    S: Scope,
    C: Callback,
{
    /// Adds a route into the global scope.
    pub fn route(self, route: impl Route) -> Builder<impl Scope<Error = Error>, C> {
        Builder {
            callback: self.callback,
            config: self.config,
            scope: self.scope.route(route),
        }
    }

    /// Creates a new scope onto the global scope using the specified `Scope`.
    pub fn mount(self, scope: impl Scope) -> Builder<impl Scope<Error = Error>, C> {
        Builder {
            callback: self.callback,
            config: self.config,
            scope: self.scope.mount(scope),
        }
    }

    /// Merges the specified `Scope` into the global scope, *without* creating a new scope.
    pub fn with(self, scope: impl Scope) -> Builder<impl Scope<Error = Error>, C> {
        Builder {
            callback: self.callback,
            config: self.config,
            scope: self.scope.with(scope),
        }
    }

    /// Adds a *global* variable into the application.
    pub fn state<T>(self, state: T) -> Builder<impl Scope<Error = S::Error>, C>
    where
        T: Send + Sync + 'static,
    {
        Builder {
            callback: self.callback,
            config: self.config,
            scope: self.scope.state(state),
        }
    }

    /// Register a `Modifier` into the global scope.
    pub fn modifier<M>(self, modifier: M) -> Builder<impl Scope<Error = S::Error>, C>
    where
        M: Modifier + Send + Sync + 'static,
    {
        Builder {
            callback: self.callback,
            config: self.config,
            scope: self.scope.modifier(modifier),
        }
    }

    pub fn prefix(self, prefix: Uri) -> Builder<impl Scope<Error = S::Error>, C> {
        Builder {
            callback: self.callback,
            config: self.config,
            scope: self.scope.prefix(prefix),
        }
    }

    /// Specifies whether to use the fallback `HEAD` handlers if it is not registered.
    ///
    /// The default value is `true`.
    pub fn fallback_head(mut self, enabled: bool) -> Builder<S, C> {
        self.config.fallback_head = enabled;
        self
    }

    /// Specifies whether to use the default `OPTIONS` handlers if it is not registered.
    ///
    /// The default value is `true`.
    pub fn fallback_options(mut self, enabled: bool) -> Builder<S, C> {
        self.config.fallback_options = enabled;
        self
    }

    pub fn on_init<F, Bd>(self, on_init: F) -> Builder<S, impl Callback>
    where
        F: Fn(&mut super::callback::Context<'_>) -> crate::error::Result<Option<Response<Bd>>>
            + Send
            + Sync
            + 'static,
        Bd: Into<ResponseBody>,
    {
        Builder {
            scope: self.scope,
            config: self.config,
            callback: {
                #[allow(missing_debug_implementations)]
                struct WrapOnInit<C, F>(C, F);

                impl<C, F, Bd> Callback for WrapOnInit<C, F>
                where
                    C: Callback,
                    F: Fn(&mut super::callback::Context<'_>)
                            -> crate::error::Result<Option<Response<Bd>>>
                        + Send
                        + Sync
                        + 'static,
                    Bd: Into<ResponseBody>,
                {
                    fn on_init(
                        &self,
                        cx: &mut super::callback::Context<'_>,
                    ) -> crate::error::Result<Option<Output>> {
                        match self.0.on_init(cx)? {
                            Some(output) => Ok(Some(output)),
                            None => {
                                (self.1)(cx).map(|x| x.map(|response| response.map(Into::into)))
                            }
                        }
                    }

                    fn on_error(
                        &self,
                        err: crate::error::Error,
                        cx: &mut super::callback::Context<'_>,
                    ) -> std::result::Result<Output, Critical> {
                        self.0.on_error(err, cx)
                    }
                }

                WrapOnInit(self.callback, on_init)
            },
        }
    }

    pub fn on_error<F, Bd>(self, on_error: F) -> Builder<S, impl Callback>
    where
        F: Fn(crate::error::Error, &mut super::callback::Context<'_>)
                -> std::result::Result<Response<Bd>, crate::error::Critical>
            + Send
            + Sync
            + 'static,
        Bd: Into<ResponseBody>,
    {
        Builder {
            scope: self.scope,
            config: self.config,
            callback: {
                #[allow(missing_debug_implementations)]
                struct WrapOnError<C, F>(C, F);

                impl<C, F, Bd> Callback for WrapOnError<C, F>
                where
                    C: Callback,
                    F: Fn(crate::error::Error, &mut super::callback::Context<'_>)
                            -> std::result::Result<Response<Bd>, Critical>
                        + Send
                        + Sync
                        + 'static,
                    Bd: Into<ResponseBody>,
                {
                    fn on_init(
                        &self,
                        cx: &mut super::callback::Context<'_>,
                    ) -> crate::error::Result<Option<Output>> {
                        self.0.on_init(cx)
                    }

                    fn on_error(
                        &self,
                        err: crate::error::Error,
                        cx: &mut super::callback::Context<'_>,
                    ) -> std::result::Result<Output, Critical> {
                        (self.1)(err, cx).map(|response| response.map(Into::into))
                    }
                }

                WrapOnError(self.callback, on_error)
            },
        }
    }

    pub fn callback<C2>(self, callback: C2) -> Builder<S, C2>
    where
        C2: Callback,
    {
        Builder {
            scope: self.scope,
            config: self.config,
            callback,
        }
    }

    /// Creates an `App` using the current configuration.
    pub fn build(self) -> Result<App> {
        build(self.scope, self.callback, self.config)
    }

    /// Creates a builder of HTTP server using the current configuration.
    pub fn build_server(self) -> Result<crate::server::Server<App>> {
        self.build().map(crate::server::Server::new)
    }
}

fn build(scope: impl Scope, callback: impl Callback, config: Config) -> Result<App> {
    let mut cx = AppContext {
        routes: vec![],
        scopes: vec![],
        modifiers: vec![],
        states: ScopedContainerBuilder::default(),
        prefix: None,
    };
    scope
        .configure(&mut ScopeContext::new(&mut cx, ScopeId::Global))
        .map_err(Into::into)?;

    let AppContext {
        routes,
        scopes,
        modifiers,
        states,
        prefix,
    } = cx;

    // finalize endpoints based on the created scope information.
    let routes: Vec<RouteData> = routes
        .into_iter()
        .enumerate()
        .map(|(route_id, route)| -> Result<RouteData> {
            // build absolute URI.
            let mut uris = vec![&route.uri];
            let mut current = route.scope_id.local_id();
            while let Some(scope) = current.and_then(|i| scopes.get(i)) {
                uris.extend(scope.prefix.as_ref());
                current = scope.parent.local_id();
            }
            uris.extend(prefix.as_ref());
            let uri = crate::uri::join_all(uris.into_iter().rev())?;

            let handler = route.handler;

            // calculate the modifier identifiers.
            let mut modifier_ids: Vec<_> = (0..modifiers.len())
                .map(|i| ModifierId(ScopeId::Global, i))
                .collect();
            if let Some(scope) = route.scope_id.local_id().and_then(|id| scopes.get(id)) {
                for (id, scope) in scope.chain.iter().filter_map(|&id| {
                    id.local_id()
                        .and_then(|id| scopes.get(id).map(|scope| (id, scope)))
                }) {
                    modifier_ids.extend(
                        (0..scope.modifiers.len()).map(|pos| ModifierId(ScopeId::Local(id), pos)),
                    );
                }
            }

            let id = RouteId(route.scope_id, route_id);

            let mut methods = route.methods;
            if methods.is_empty() {
                methods.insert(Method::GET);
            }

            Ok(RouteData {
                id,
                uri,
                methods,
                handler,
                modifier_ids,
            })
        }).collect::<std::result::Result<_, _>>()?;

    // create a router
    let (recognizer, endpoints) = {
        let mut collected_routes = IndexMap::<Uri, IndexMap<Method, usize>>::new();
        for (i, route) in routes.iter().enumerate() {
            let methods = collected_routes
                .entry(route.uri.clone())
                .or_insert_with(IndexMap::<Method, usize>::new);

            for method in &route.methods {
                if methods.contains_key(method) {
                    return Err(Error::from(failure::format_err!(
                        "Adding routes with duplicate URI and method is currenly not supported. \
                         (uri={}, method={})",
                        route.uri,
                        method
                    )));
                }

                methods.insert(method.clone(), i);
            }
        }

        log::debug!("collected routes:");
        for (uri, methods) in &collected_routes {
            log::debug!(" - {} {:?}", uri, methods.keys().collect::<Vec<_>>());
        }

        let mut recognizer = Recognizer::default();
        let mut endpoints = vec![];
        for (uri, methods) in collected_routes {
            let allowed_methods = {
                let allowed_methods: IndexSet<_> =
                    methods.keys().chain(Some(&Method::OPTIONS)).collect();
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

            recognizer.add_route(uri)?;
            endpoints.push(EndpointData {
                route_ids: methods,
                allowed_methods,
            });
        }

        (recognizer, endpoints)
    };

    // finalize global/scope-local storages.
    let parents: Vec<_> = scopes.iter().map(|scope| scope.parent).collect();
    let states = states.finish(&parents[..]);

    let scopes = scopes
        .into_iter()
        .map(|scope| ScopeData {
            id: scope.id,
            parent: scope.parent,
            prefix: scope.prefix,
            modifiers: scope.modifiers,
        }).collect();

    Ok(App {
        data: Arc::new(AppData {
            routes,
            scopes,
            global_scope: ScopeData {
                id: ScopeId::Global,
                parent: ScopeId::Global, // dummy
                prefix,
                modifiers,
            },
            recognizer,
            endpoints,
            config,
            callback: Box::new(callback),
            states,
        }),
    })
}

#[allow(missing_debug_implementations)]
pub struct AppContext {
    routes: Vec<RouteBuilder>,
    scopes: Vec<ScopeBuilder>,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    states: ScopedContainerBuilder,
    prefix: Option<Uri>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for AppContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppContext")
            .field("routes", &self.routes)
            .field("scopes", &self.scopes)
            .field("states", &self.states)
            .field("prefix", &self.prefix)
            .finish()
    }
}

impl AppContext {
    pub(super) fn new_route(&mut self, scope_id: ScopeId, route: impl Route) -> Result<()> {
        let mut cx = RouteContext {
            uri: Uri::root(),
            methods: None,
            handler: None,
        };
        route.configure(&mut cx);

        let route = RouteBuilder {
            scope_id,
            methods: cx
                .methods
                .unwrap_or_else(|| vec![Method::GET].into_iter().collect()),
            uri: cx.uri,
            handler: cx
                .handler
                .ok_or_else(|| failure::format_err!("default handler is not supported"))?,
        };
        self.routes.push(route);

        Ok(())
    }

    pub(super) fn new_scope(&mut self, parent: ScopeId, scope: impl Scope) -> Result<()> {
        let id = ScopeId::Local(self.scopes.len());
        let mut chain = parent
            .local_id()
            .map_or_else(Default::default, |id| self.scopes[id].chain.clone());
        chain.push(id);
        self.scopes.push(ScopeBuilder {
            id,
            parent,
            prefix: None,
            modifiers: vec![],
            chain,
        });

        scope
            .configure(&mut ScopeContext::new(self, id))
            .map_err(Into::into)?;

        Ok(())
    }

    pub(super) fn add_modifier<M>(&mut self, id: ScopeId, modifier: M)
    where
        M: Modifier + Send + Sync + 'static,
    {
        match id {
            ScopeId::Global => self.modifiers.push(Box::new(modifier)),
            ScopeId::Local(id) => self.scopes[id].modifiers.push(Box::new(modifier)),
        }
    }

    pub(super) fn set_state<T>(&mut self, value: T, id: ScopeId)
    where
        T: Send + Sync + 'static,
    {
        self.states.set(value, id);
    }

    pub(super) fn set_prefix(&mut self, id: ScopeId, prefix: Uri) {
        match id {
            ScopeId::Global => self.prefix = Some(prefix),
            ScopeId::Local(id) => self.scopes[id].prefix = Some(prefix),
        }
    }
}

struct RouteBuilder {
    scope_id: ScopeId,
    methods: IndexSet<Method>,
    uri: Uri,
    handler: Box<dyn Handler + Send + Sync + 'static>,
}

impl fmt::Debug for RouteBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteBuilder")
            .field("scope_id", &self.scope_id)
            .field("methods", &self.methods)
            .field("uri", &self.uri)
            .finish()
    }
}

struct ScopeBuilder {
    id: ScopeId,
    parent: ScopeId,
    modifiers: Vec<Box<dyn Modifier + Send + Sync + 'static>>,
    prefix: Option<Uri>,
    chain: Vec<ScopeId>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for ScopeBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeBuilder")
            .field("parent", &self.parent)
            .field("prefix", &self.prefix)
            .field("chain", &self.chain)
            .finish()
    }
}
