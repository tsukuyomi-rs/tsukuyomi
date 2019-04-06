use {
    super::{
        concurrency::{Concurrency, DefaultConcurrency},
        error::{Error, Result},
        path::{Path, PathExtractor},
        recognizer::Recognizer,
        scope::{ScopeId, Scopes},
        App, AppInner, ResourceData, RouteData, ScopeData, Uri,
    },
    crate::{
        endpoint::Endpoint, extractor::Extractor, generic::Combine, handler::ModifyHandler,
        util::Chain,
    },
    http::Method,
    indexmap::map::{Entry, IndexMap},
    std::{marker::PhantomData, rc::Rc, sync::Arc},
};

#[derive(Debug)]
pub struct Builder<C: Concurrency = DefaultConcurrency> {
    app: AppInner<C>,
    _anchor: PhantomData<Rc<()>>,
}

impl<C> Builder<C>
where
    C: Concurrency,
{
    /// Creates a new `Builder`.
    pub fn new() -> Self {
        Self {
            app: AppInner {
                recognizer: Recognizer::default(),
                scopes: Scopes::new(ScopeData {
                    prefix: Uri::root(),
                    fallback: None,
                }),
            },
            _anchor: PhantomData,
        }
    }

    /// Configures the root scope using the specified function.
    pub fn root<F>(mut self, f: F) -> Result<Self>
    where
        F: FnOnce(Scope<'_, (), C>) -> Result<()>,
    {
        f(Scope {
            app: &mut self,
            scope_id: ScopeId::root(),
            modifier: (),
        })?;
        Ok(self)
    }

    fn new_scope<P: AsRef<str>, M>(
        &mut self,
        prefix: P,
        scope_id: ScopeId,
        modifier: M,
    ) -> Result<Scope<'_, M, C>> {
        let prefix: Uri = prefix.as_ref().parse().map_err(Error::custom)?;

        let scope_id = self
            .app
            .scopes
            .add_node(scope_id, {
                let parent = &self.app.scopes[scope_id].data;
                ScopeData {
                    prefix: parent.prefix.join(&prefix).map_err(Error::custom)?,
                    fallback: None,
                }
            })
            .map_err(Error::custom)?;

        Ok(Scope {
            app: self,
            scope_id,
            modifier,
        })
    }

    fn new_resource<P: Path, M>(
        &mut self,
        path: P,
        scope_id: ScopeId,
        modifier: M,
    ) -> Result<Resource<'_, P, M, C>> {
        let uri: Uri = path.as_str().parse().map_err(Error::custom)?;
        let uri = self.app.scopes[scope_id]
            .data
            .prefix
            .join(&uri)
            .map_err(Error::custom)?;

        let scope = &self.app.scopes[scope_id];

        let resource = self
            .app
            .recognizer
            .insert(
                uri.as_str(),
                Arc::new(ResourceData {
                    scope: scope.id(),
                    ancestors: scope
                        .ancestors()
                        .iter()
                        .cloned()
                        .chain(Some(scope.id()))
                        .collect(),
                    uri: uri.clone(),
                    routes: vec![],
                    default_route: None,
                    verbs: IndexMap::default(),
                }),
            )
            .map_err(Error::custom)?;

        Ok(Resource {
            resource: Arc::get_mut(resource).expect("the instance has already been shared"),
            modifier,
            path,
        })
    }

    fn set_fallback(&mut self, scope_id: ScopeId, handler: C::Handler) {
        self.app.scopes[scope_id].data.fallback = Some(handler);
    }

    /// Construct an `App` using the current configuration.
    pub fn build(self) -> Result<App<C>> {
        Ok(App {
            inner: Arc::new(self.app),
        })
    }
}

/// A type representing the "scope" in Web application.
#[derive(Debug)]
pub struct Scope<'a, M, C: Concurrency = DefaultConcurrency> {
    app: &'a mut Builder<C>,
    scope_id: ScopeId,
    modifier: M,
}

/// The experimental API for the next version.
impl<'a, M, C> Scope<'a, M, C>
where
    C: Concurrency,
{
    /// Creates a resource that has the provided path.
    pub fn at<P>(&mut self, path: P) -> Result<Resource<'_, P, &M, C>>
    where
        P: Path,
    {
        self.app.new_resource(path, self.scope_id, &self.modifier)
    }

    /// Registers the scope-level fallback handler onto the current scope.
    ///
    /// The fallback handler is called when there are no resources that exactly
    /// matches to the incoming request.
    pub fn fallback<T>(&mut self, endpoint: T) -> Result<()>
    where
        T: Endpoint<()>,
        M: ModifyHandler<EndpointHandler<(), T>>,
        M::Handler: Into<C::Handler>,
    {
        let handler = EndpointHandler::new(endpoint, ());
        let handler = self.modifier.modify(handler);
        self.app.set_fallback(self.scope_id, handler.into());
        Ok(())
    }

    /// Creates a sub-scope onto the current scope.
    pub fn mount<P>(&mut self, prefix: P) -> Result<Scope<'_, &M, C>>
    where
        P: AsRef<str>,
    {
        self.app.new_scope(prefix, self.scope_id, &self.modifier)
    }

    /// Adds the provided `ModifyHandler` to the stack and executes a configuration.
    ///
    /// Unlike `nest`, this method does not create a scope.
    pub fn with<M2>(&mut self, modifier: M2) -> Scope<'_, Chain<M2, &M>, C> {
        Scope {
            app: &mut *self.app,
            scope_id: self.scope_id,
            modifier: Chain::new(modifier, &self.modifier),
        }
    }

    /// Applies itself to the provided function.
    pub fn done<F, T>(self, f: F) -> Result<T>
    where
        F: FnOnce(Self) -> Result<T>,
    {
        f(self)
    }
}

/// A resource associated with a specific HTTP path.
#[derive(Debug)]
pub struct Resource<'s, P, M, C>
where
    P: Path,
    C: Concurrency,
{
    resource: &'s mut ResourceData<C>,
    path: P,
    modifier: M,
}

impl<'s, P, M, C> Resource<'s, P, M, C>
where
    P: Path,
    C: Concurrency,
{
    /// Creates a `Route` that matches to the specified HTTP methods.
    pub fn route(
        &mut self,
        methods: impl IntoIterator<Item = impl Into<Method>>,
    ) -> Route<'_, PathExtractor<P>, &M, C> {
        self.route2(Some(methods.into_iter().map(Into::into).collect()))
    }

    fn route2(&mut self, methods: Option<Vec<Method>>) -> Route<'_, PathExtractor<P>, &M, C> {
        Route {
            resource: &mut *self.resource,
            methods,
            modifier: &self.modifier,
            extractor: PathExtractor::<P>::new(),
        }
    }

    pub fn get(&mut self) -> Route<'_, PathExtractor<P>, &M, C> {
        self.route(Some(Method::GET))
    }

    pub fn post(&mut self) -> Route<'_, PathExtractor<P>, &M, C> {
        self.route(Some(Method::POST))
    }

    pub fn put(&mut self) -> Route<'_, PathExtractor<P>, &M, C> {
        self.route(Some(Method::PUT))
    }

    pub fn head(&mut self) -> Route<'_, PathExtractor<P>, &M, C> {
        self.route(Some(Method::HEAD))
    }

    pub fn delete(&mut self) -> Route<'_, PathExtractor<P>, &M, C> {
        self.route(Some(Method::DELETE))
    }

    pub fn patch(&mut self) -> Route<'_, PathExtractor<P>, &M, C> {
        self.route(Some(Method::PATCH))
    }

    /// Start building of a `Route` that matches any HTTP method.
    pub fn any(&mut self) -> Route<'_, PathExtractor<P>, &M, C> {
        self.route2(None)
    }

    pub fn to<T>(&mut self, endpoint: T) -> Result<()>
    where
        T: Endpoint<P::Output>,
        M: ModifyHandler<EndpointHandler<PathExtractor<P>, T>>,
        M::Handler: Into<C::Handler>,
    {
        self.any().to(endpoint)
    }

    /// Appends a `ModifyHandler` to the stack applied to the all handlers on this resource.
    pub fn with<M2>(self, modifier: M2) -> Resource<'s, P, Chain<M2, M>, C> {
        Resource {
            resource: self.resource,
            path: self.path,
            modifier: Chain::new(modifier, self.modifier),
        }
    }

    /// Applies itself to the specified function.
    pub fn done<F, T>(self, f: F) -> Result<T>
    where
        F: FnOnce(Self) -> Result<T>,
    {
        f(self)
    }
}

#[allow(missing_debug_implementations)]
pub struct Route<'a, E, M, C>
where
    E: Extractor,
    C: Concurrency,
{
    resource: &'a mut ResourceData<C>,
    methods: Option<Vec<Method>>,
    extractor: E,
    modifier: M,
}

impl<'a, E, M, C> Route<'a, E, M, C>
where
    E: Extractor,
    C: Concurrency,
{
    pub fn with<M2>(self, modifier: M2) -> Route<'a, E, Chain<M2, M>, C> {
        Route {
            resource: self.resource,
            methods: self.methods,
            modifier: Chain::new(modifier, self.modifier),
            extractor: self.extractor,
        }
    }

    pub fn extract<E2>(self, extractor: E2) -> Route<'a, Chain<E, E2>, M, C>
    where
        E2: Extractor,
        E::Output: Combine<E2::Output>,
    {
        Route {
            resource: self.resource,
            methods: self.methods,
            modifier: self.modifier,
            extractor: Chain::new(self.extractor, extractor),
        }
    }

    pub fn to<T>(self, endpoint: T) -> Result<()>
    where
        T: Endpoint<E::Output>,
        M: ModifyHandler<EndpointHandler<E, T>>,
        M::Handler: Into<C::Handler>,
    {
        let handler = self
            .modifier
            .modify(EndpointHandler::new(endpoint, self.extractor));
        let route = RouteData {
            handler: handler.into(),
        };

        if let Some(methods) = self.methods {
            let index = self.resource.routes.len();
            self.resource.routes.push(route);

            for method in methods {
                match self.resource.verbs.entry(method) {
                    Entry::Occupied(..) => {
                        return Err(Error::custom(failure::format_err!("duplicated method")));
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(index);
                    }
                }
            }
        } else {
            if self.resource.default_route.is_some() {
                return Err(Error::custom(failure::format_err!(
                    "the default route handler has already been set"
                )));
            }
            self.resource.default_route = Some(route);
        }
        Ok(())
    }
}

/// A `Handler` that uses on an endpoint tied to a specific HTTP path.
#[allow(missing_debug_implementations)]
pub struct EndpointHandler<E, T> {
    endpoint: Arc<T>,
    extractor: E,
}

impl<E, T> EndpointHandler<E, T>
where
    E: Extractor,
    T: Endpoint<E::Output>,
{
    pub(crate) fn new(endpoint: T, extractor: E) -> Self {
        Self {
            endpoint: Arc::new(endpoint),
            extractor,
        }
    }
}

mod handler {
    use {
        super::EndpointHandler,
        crate::{
            endpoint::Endpoint,
            error::Error,
            extractor::Extractor,
            future::{Poll, TryFuture},
            handler::Handler,
            input::Input,
        },
        std::sync::Arc,
    };

    impl<E, T> Handler for EndpointHandler<E, T>
    where
        E: Extractor,
        T: Endpoint<E::Output>,
    {
        type Output = T::Output;
        type Error = Error;
        type Handle = EndpointHandle<E, T>;

        fn handle(&self) -> Self::Handle {
            EndpointHandle {
                state: State::Init(self.endpoint.clone(), self.extractor.extract()),
            }
        }
    }

    #[doc(hidden)]
    #[allow(missing_debug_implementations)]
    pub struct EndpointHandle<E, T>
    where
        E: Extractor,
        T: Endpoint<E::Output>,
    {
        state: State<E, T>,
    }

    #[allow(missing_debug_implementations)]
    enum State<E, T>
    where
        E: Extractor,
        T: Endpoint<E::Output>,
    {
        Init(Arc<T>, E::Extract),
        InFlight(T::Future),
    }

    impl<E, T> TryFuture for EndpointHandle<E, T>
    where
        E: Extractor,
        T: Endpoint<E::Output>,
    {
        type Ok = T::Output;
        type Error = Error;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            loop {
                self.state = match self.state {
                    State::Init(ref endpoint, ref mut extract) => {
                        let args =
                            futures01::try_ready!(extract.poll_ready(input).map_err(Into::into));
                        State::InFlight(endpoint.apply(args))
                    }
                    State::InFlight(ref mut in_flight) => {
                        return in_flight.poll_ready(input).map_err(Into::into);
                    }
                };
            }
        }
    }
}
