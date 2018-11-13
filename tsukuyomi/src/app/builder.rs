use std::fmt;
use std::sync::Arc;

use bytes::BytesMut;
use http::header;
use http::header::HeaderValue;
use http::{Method, Response};
use indexmap::{IndexMap, IndexSet};

use crate::error::handler::DefaultErrorHandler;
use crate::error::ErrorHandler;
use crate::internal::recognizer::Recognizer;
use crate::internal::scoped_map::{Builder as ScopedContainerBuilder, ScopeId};
use crate::internal::uri;
use crate::internal::uri::Uri;
use crate::output::ResponseBody;

use super::error::{Error, Result};
use super::global::{Context as GlobalContext, Global};
use super::handler::{AsyncResult, Handler, Modifier};
use super::route::{Context as RouteContext, Route};
use super::scope::{Context as ScopeContext, Scope};
use super::{App, AppData, Config, ModifierId, RouteData, RouteId, ScopeData};

pub fn build(scope: impl Scope, global: impl Global) -> Result<App> {
    let mut cx = AppContext {
        routes: vec![],
        scopes: vec![],
        config: Config::default(),
        error_handler: None,
        modifiers: vec![],
        states: ScopedContainerBuilder::default(),
        prefix: None,
    };
    scope
        .configure(&mut ScopeContext::new(&mut cx, ScopeId::Global))
        .map_err(Into::into)?;
    global.configure(&mut GlobalContext::new(&mut cx));

    let AppContext {
        routes,
        scopes,
        config,
        error_handler,
        modifiers,
        states,
        prefix,
    } = cx;

    // finalize endpoints based on the created scope information.
    let mut routes: Vec<RouteData> = routes
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
            let uri = uri::join_all(uris.into_iter().rev())?;

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
    let (recognizer, route_ids) = {
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
        let mut route_ids = vec![];
        for (uri, mut methods) in collected_routes {
            if config.fallback_options {
                let m = methods
                    .keys()
                    .cloned()
                    .chain(Some(Method::OPTIONS))
                    .collect();
                methods.entry(Method::OPTIONS).or_insert_with(|| {
                    let id = routes.len();
                    routes.push(RouteData {
                        id: RouteId(ScopeId::Global, id),
                        uri: uri.clone(),
                        methods: vec![Method::OPTIONS].into_iter().collect(),
                        handler: default_options_handler(m),
                        modifier_ids: (0..modifiers.len())
                            .map(|i| ModifierId(ScopeId::Global, i))
                            .collect(),
                    });
                    id
                });
            }

            recognizer.add_route(uri)?;
            route_ids.push(methods);
        }

        (recognizer, route_ids)
    };

    // finalize error handler.
    let error_handler = error_handler.unwrap_or_else(|| Box::new(DefaultErrorHandler::default()));

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
            route_ids,
            config,
            error_handler,
            states,
        }),
    })
}

#[allow(missing_debug_implementations)]
pub struct AppContext {
    routes: Vec<RouteBuilder>,
    scopes: Vec<ScopeBuilder>,
    config: Config,
    error_handler: Option<Box<dyn ErrorHandler + Send + Sync + 'static>>,
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
            .field("config", &self.config)
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

    pub(super) fn fallback_head(&mut self, enabled: bool) {
        self.config.fallback_head = enabled;
    }

    pub(super) fn fallback_options(&mut self, enabled: bool) {
        self.config.fallback_options = enabled;
    }

    /// Sets the instance to an error handler into this builder.
    pub(super) fn set_error_handler<E>(&mut self, error_handler: E)
    where
        E: ErrorHandler + Send + Sync + 'static,
    {
        self.error_handler = Some(Box::new(error_handler));
    }

    pub(super) fn set_prefix(&mut self, id: ScopeId, prefix: &str) -> Result<()> {
        let prefix = prefix.parse().unwrap();
        match id {
            ScopeId::Global => self.prefix = Some(prefix),
            ScopeId::Local(id) => self.scopes[id].prefix = Some(prefix),
        }
        Ok(())
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

fn default_options_handler(methods: Vec<Method>) -> Box<dyn Handler + Send + Sync + 'static> {
    let allowed_methods = {
        let bytes = methods
            .into_iter()
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

    Box::new(super::handler::raw(move |_| {
        let mut response = Response::new(ResponseBody::empty());
        response
            .headers_mut()
            .insert(header::ALLOW, allowed_methods.clone());
        AsyncResult::ready(Ok(response))
    }))
}
