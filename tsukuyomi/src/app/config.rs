use {
    super::{
        concurrency::{current_thread::CurrentThread, Concurrency, DefaultConcurrency},
        recognizer::Recognizer,
        scope::{ScopeId, Scopes},
        App, AppInner, Endpoint, ScopeData, Uri,
    },
    crate::{
        handler::{Handler, ModifyHandler},
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

impl App {
    /// Creates a new `App` from the provided configuration.
    pub fn create(config: impl Config<()>) -> Result<Self> {
        App::create_imp(config)
    }
}

impl App<CurrentThread> {
    /// Creates a new `App` from the provided configuration, without guarantees of thread safety.
    pub fn create_local(config: impl Config<(), CurrentThread>) -> Result<Self> {
        App::create_imp(config)
    }
}

impl<C> App<C>
where
    C: Concurrency,
{
    /// Creates a new `App` from the provided configuration.
    fn create_imp(config: impl Config<(), C>) -> Result<Self> {
        let mut recognizer = Recognizer::default();
        let mut scopes = Scopes::new(ScopeData {
            prefix: Uri::root(),
            default_handler: None,
        });
        config
            .configure(&mut Scope {
                recognizer: &mut recognizer,
                scopes: &mut scopes,
                scope_id: ScopeId::root(),
                modifier: &(),
                _marker: PhantomData,
            })
            .map_err(Into::into)?;

        Ok(Self {
            inner: Arc::new(AppInner { recognizer, scopes }),
        })
    }
}

/// A type representing the contextual information in `Config::configure`.
#[derive(Debug)]
pub struct Scope<'a, M, C: Concurrency = DefaultConcurrency> {
    recognizer: &'a mut Recognizer<Arc<Endpoint<C>>>,
    scopes: &'a mut Scopes<ScopeData<C>>,
    modifier: &'a M,
    scope_id: ScopeId,
    _marker: PhantomData<Rc<()>>,
}

impl<'a, M, C> Scope<'a, M, C>
where
    C: Concurrency,
{
    /// Adds a route onto the current scope.
    pub fn route<H>(&mut self, handler: H) -> Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Into<C::Handler>,
    {
        let handler = self.modifier.modify(handler);

        if let Some(path) = handler.metadata().path().cloned() {
            let uri = self.scopes[self.scope_id]
                .data
                .prefix
                .join(&path)
                .map_err(Error::custom)?;

            let scope = &self.scopes[self.scope_id];
            self.recognizer
                .insert(
                    uri.as_str(),
                    Arc::new(Endpoint {
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
            self.scopes[self.scope_id].data.default_handler = Some(handler.into());
        }

        Ok(())
    }

    /// Creates a sub-scope with the provided prefix onto the current scope.
    pub fn mount(&mut self, prefix: impl AsRef<str>, config: impl Config<M, C>) -> Result<()> {
        let prefix: Uri = prefix.as_ref().parse().map_err(Error::custom)?;

        let scope_id = self
            .scopes
            .add_node(self.scope_id, {
                let parent = &self.scopes[self.scope_id].data;
                ScopeData {
                    prefix: parent.prefix.join(&prefix).map_err(Error::custom)?,
                    default_handler: None,
                }
            })
            .map_err(Error::custom)?;

        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id,
                modifier: &*self.modifier,
                _marker: PhantomData,
            })
            .map_err(Into::into)?;

        Ok(())
    }

    /// Applies the specified configuration with a `ModifyHandler` on the current scope.
    pub fn modify<M2>(
        &mut self,
        modifier: M2,
        config: impl Config<Chain<&'a M, M2>, C>,
    ) -> Result<()> {
        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id: self.scope_id,
                modifier: &Chain::new(self.modifier, modifier),
                _marker: PhantomData,
            })
            .map_err(Into::into)
    }
}

/// A marker trait annotating that the implementator has an implementation of `Config<M, C>`
/// for a certain `M` and `C`.
pub trait IsConfig {}

/// A trait that abstracts the configuring for constructing an instance of `App`.
pub trait Config<M, C: Concurrency = DefaultConcurrency>: IsConfig {
    type Error: Into<Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut Scope<'_, M, C>) -> std::result::Result<(), Self::Error>;
}

impl<T1, T2> IsConfig for Chain<T1, T2>
where
    T1: IsConfig,
    T2: IsConfig,
{
}

impl<S1, S2, M, C> Config<M, C> for Chain<S1, S2>
where
    S1: Config<M, C>,
    S2: Config<M, C>,
    C: Concurrency,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M, C>) -> std::result::Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

impl<T> IsConfig for Option<T> where T: IsConfig {}

impl<M, S, C> Config<M, C> for Option<S>
where
    S: Config<M, C>,
    C: Concurrency,
{
    type Error = S::Error;

    fn configure(self, cx: &mut Scope<'_, M, C>) -> std::result::Result<(), Self::Error> {
        if let Some(scope) = self {
            scope.configure(cx)?;
        }
        Ok(())
    }
}

impl<T, E> IsConfig for std::result::Result<T, E>
where
    T: IsConfig,
    E: Into<Error>,
{
}

impl<M, S, E, C> Config<M, C> for std::result::Result<S, E>
where
    S: Config<M, C>,
    E: Into<Error>,
    C: Concurrency,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M, C>) -> std::result::Result<(), Self::Error> {
        self.map_err(Into::into)?.configure(cx).map_err(Into::into)
    }
}

impl IsConfig for () {}

impl<M, C> Config<M, C> for ()
where
    C: Concurrency,
{
    type Error = Never;

    fn configure(self, _: &mut Scope<'_, M, C>) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}
