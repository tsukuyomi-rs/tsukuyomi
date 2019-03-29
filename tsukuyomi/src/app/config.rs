use {
    super::{
        concurrency::{Concurrency, DefaultConcurrency},
        path::{IntoPath, Path, PathExtractor},
        recognizer::Recognizer,
        scope::{ScopeId, Scopes},
        App, AppInner, ResourceData, ScopeData, Uri,
    },
    crate::{
        endpoint::Endpoint,
        handler::{metadata::Metadata, Handler, ModifyHandler},
        util::{Chain, Never},
    },
    std::{error, fmt, marker::PhantomData, rc::Rc, sync::Arc},
};

/// A type alias of `Result<T, E>` whose error type is restricted to `AppError`.
pub type Result<T> = std::result::Result<T, Error>;

/// An error type which will be thrown from `AppBuilder`.
#[derive(Debug)]
pub struct Error {
    cause: failure::Compat<failure::Error>,
}

impl From<Never> for Error {
    fn from(never: Never) -> Self {
        match never {}
    }
}

impl Error {
    pub fn custom<E>(cause: E) -> Self
    where
        E: Into<failure::Error>,
    {
        Self {
            cause: cause.into().compat(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(f)
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(&self.cause)
    }
}

impl<C> App<C>
where
    C: Concurrency,
{
    /// Construct an `App` using the provided function.
    pub fn build<F>(f: F) -> Result<Self>
    where
        F: FnOnce(&mut Scope<'_, (), C>) -> Result<()>,
    {
        let mut app = AppInner {
            recognizer: Recognizer::default(),
            scopes: Scopes::new(ScopeData {
                prefix: Uri::root(),
                default_handler: None,
            }),
        };

        f(&mut Scope {
            app: &mut app,
            scope_id: ScopeId::root(),
            modifier: &(),
            _marker: PhantomData,
        })?;

        Ok(Self {
            inner: Arc::new(app),
        })
    }
}

/// A type representing the "scope" in Web application.
#[derive(Debug)]
pub struct Scope<'a, M, C: Concurrency = DefaultConcurrency> {
    app: &'a mut AppInner<C>,
    modifier: &'a M,
    scope_id: ScopeId,
    _marker: PhantomData<Rc<()>>,
}

impl<'a, M, C> Scope<'a, M, C>
where
    C: Concurrency,
{
    pub(crate) fn route2<H>(&mut self, handler: H) -> Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Into<C::Handler>,
    {
        let handler = self.modifier.modify(handler);

        if let Some(path) = handler.metadata().path().cloned() {
            let uri = self.app.scopes[self.scope_id]
                .data
                .prefix
                .join(&path)
                .map_err(Error::custom)?;

            let scope = &self.app.scopes[self.scope_id];
            self.app
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
                        handler: handler.into(),
                    }),
                )
                .map_err(Error::custom)?;
        } else {
            self.app.scopes[self.scope_id].data.default_handler = Some(handler.into());
        }

        Ok(())
    }
}

/// The experimental API for the next version.
impl<'a, M, C> Scope<'a, M, C>
where
    C: Concurrency,
{
    /// Adds a route onto the current scope.
    pub fn at<P, M2, T>(&mut self, path: P, modifier: M2, endpoint: T) -> Result<()>
    where
        P: IntoPath,
        T: Endpoint<P::Output>,
        M2: ModifyHandler<RouteHandler<P::Extractor, T>>,
        M: ModifyHandler<M2::Handler>,
        M::Handler: Into<C::Handler>,
    {
        let handler = RouteHandler::new(path.into_path(), endpoint);
        self.route2(modifier.modify(handler))
    }

    /// Adds a default route onto the current scope.
    ///
    /// The default route is used when the incoming request URI matches the prefix
    /// of the current scope and there are no route that exactly matches.
    pub fn default<M2, T>(&mut self, modifier: M2, endpoint: T) -> Result<()>
    where
        T: Endpoint<()>,
        M2: ModifyHandler<RouteHandler<(), T>>,
        M: ModifyHandler<M2::Handler>,
        M::Handler: Into<C::Handler>,
    {
        self.at(Path::<()>::new("*"), modifier, endpoint)
    }

    /// Creates a sub-scope onto the current scope.
    ///
    /// Calling `nest` constructs a scope and allows to set a *default* route
    /// that partially matches up to the prefix.
    pub fn nest<P, M2, F>(&mut self, prefix: P, modifier: M2, f: F) -> Result<()>
    where
        P: AsRef<str>,
        F: FnOnce(&mut Scope<'_, Chain<M2, &'a M>, C>) -> Result<()>,
    {
        let prefix: Uri = prefix.as_ref().parse().map_err(Error::custom)?;

        let scope_id = self
            .app
            .scopes
            .add_node(self.scope_id, {
                let parent = &self.app.scopes[self.scope_id].data;
                ScopeData {
                    prefix: parent.prefix.join(&prefix).map_err(Error::custom)?,
                    default_handler: None,
                }
            })
            .map_err(Error::custom)?;

        f(&mut Scope {
            app: &mut *self.app,
            scope_id,
            modifier: &Chain::new(modifier, self.modifier),
            _marker: PhantomData,
        })
    }

    /// Adds the provided `ModifyHandler` to the stack and executes a configuration.
    ///
    /// Unlike `nest`, this method does not create a scope.
    pub fn with<M2, F>(&mut self, modifier: M2, f: F) -> Result<()>
    where
        F: FnOnce(&mut Scope<'_, Chain<M2, &'a M>, C>) -> Result<()>,
    {
        f(&mut Scope {
            app: &mut *self.app,
            scope_id: self.scope_id,
            modifier: &Chain::new(modifier, self.modifier),
            _marker: PhantomData,
        })
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct RouteHandler<E, T> {
    endpoint: Arc<T>,
    metadata: Metadata,
    _marker: PhantomData<E>,
}

impl<E, T> RouteHandler<E, T>
where
    E: PathExtractor,
    T: Endpoint<E::Output>,
{
    pub(crate) fn new(path: Path<E>, endpoint: T) -> Self {
        let path = path.uri_str();
        let endpoint = Arc::new(endpoint);

        let mut metadata = match path {
            "*" => Metadata::without_suffix(),
            path => Metadata::new(path.parse().expect("this is a bug")),
        };
        *metadata.allowed_methods_mut() = endpoint.allowed_methods();

        Self {
            endpoint,
            metadata,
            _marker: PhantomData,
        }
    }
}

mod handle {
    use {
        super::{PathExtractor, RouteHandler},
        crate::{
            endpoint::{ApplyContext, Endpoint},
            error::Error,
            future::{Poll, TryFuture},
            handler::{metadata::Metadata, Handler},
            input::Input,
        },
        std::{marker::PhantomData, sync::Arc},
    };

    impl<E, T> Handler for RouteHandler<E, T>
    where
        E: PathExtractor,
        T: Endpoint<E::Output>,
    {
        type Output = T::Output;
        type Error = Error;
        type Handle = RouteHandle<E, T>;

        fn handle(&self) -> Self::Handle {
            RouteHandle {
                state: RouteHandleState::Init(self.endpoint.clone()),
                _marker: PhantomData,
            }
        }

        fn metadata(&self) -> Metadata {
            self.metadata.clone()
        }
    }

    #[doc(hidden)]
    #[allow(missing_debug_implementations)]
    pub struct RouteHandle<E, T>
    where
        E: PathExtractor,
        T: Endpoint<E::Output>,
    {
        state: RouteHandleState<T, T::Future>,
        _marker: PhantomData<E>,
    }

    #[allow(missing_debug_implementations)]
    enum RouteHandleState<T, Fut> {
        Init(Arc<T>),
        InFlight(Fut),
    }

    impl<E, T> TryFuture for RouteHandle<E, T>
    where
        E: PathExtractor,
        T: Endpoint<E::Output>,
    {
        type Ok = T::Output;
        type Error = Error;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            loop {
                self.state = match self.state {
                    RouteHandleState::Init(ref endpoint) => {
                        let args = E::extract(input.params.as_ref())?;
                        RouteHandleState::InFlight(
                            endpoint
                                .apply(args, &mut ApplyContext::new(input))
                                .map_err(|(_args, err)| err)?,
                        )
                    }
                    RouteHandleState::InFlight(ref mut in_flight) => {
                        return in_flight.poll_ready(input).map_err(Into::into);
                    }
                };
            }
        }
    }
}
