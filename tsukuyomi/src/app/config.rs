use {
    super::{
        recognizer::Recognizer,
        scope::{ScopeId, Scopes},
        App, AppData, AppInner, Endpoint, EndpointId, ScopeData, Uri,
    },
    crate::{
        core::{Chain, Never},
        handler::{Handler, ModifyHandler},
    },
    failure::Fail,
    std::{fmt, sync::Arc},
};

/// A type alias of `Result<T, E>` whose error type is restricted to `AppError`.
pub type Result<T> = std::result::Result<T, Error>;

/// An error type which will be thrown from `AppBuilder`.
#[derive(Debug)]
pub struct Error {
    compat: Compat,
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.compat.fmt(f)
    }
}

impl<E> From<E> for Error
where
    E: Into<failure::Error>,
{
    fn from(cause: E) -> Self {
        Self::custom(cause)
    }
}

impl Error {
    pub fn custom<E>(cause: E) -> Self
    where
        E: Into<failure::Error>,
    {
        Self {
            compat: Compat::Custom {
                cause: cause.into(),
            },
        }
    }

    pub fn compat(self) -> Compat {
        self.compat
    }
}

#[doc(hidden)]
#[derive(Debug, Fail)]
pub enum Compat {
    #[fail(display = "{}", cause)]
    Custom { cause: failure::Error },
}

/// Creates an `App` using the specified configuration.
pub(super) fn configure<T: AppData>(config: impl Config<(), T>) -> Result<App<T>> {
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
        })
        .map_err(Into::into)?;

    Ok(App {
        inner: Arc::new(AppInner { recognizer, scopes }),
    })
}

/// A type representing the contextual information in `Scope::configure`.
#[derive(Debug)]
pub struct Scope<'a, M, T: AppData> {
    recognizer: &'a mut Recognizer<Endpoint<T>>,
    scopes: &'a mut Scopes<ScopeData<T>>,
    modifier: &'a M,
    scope_id: ScopeId,
}

impl<'a, M, T> Scope<'a, M, T>
where
    T: AppData,
{
    /// Appends a `Handler` with the specified URI onto the current scope.
    pub fn route<H>(&mut self, uri: Option<impl AsRef<str>>, handler: H) -> Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Into<T::Handler>,
    {
        if let Some(uri) = uri {
            let uri: Uri = uri.as_ref().parse()?;
            let uri = self.scopes[self.scope_id].data.prefix.join(&uri)?;

            let id = EndpointId(self.recognizer.len());
            let scope = &self.scopes[self.scope_id];
            self.recognizer.insert(
                uri.as_str(),
                Endpoint {
                    id,
                    scope: scope.id(),
                    ancestors: scope
                        .ancestors()
                        .into_iter()
                        .cloned()
                        .chain(Some(scope.id()))
                        .collect(),
                    uri: uri.clone(),
                    handler: self.modifier.modify(handler).into(),
                },
            )?;
        } else {
            self.scopes[self.scope_id].data.default_handler =
                Some(self.modifier.modify(handler).into());
        }
        Ok(())
    }

    /// Creates a sub-scope with the provided prefix onto the current scope.
    pub fn mount(&mut self, prefix: impl AsRef<str>, config: impl Config<M, T>) -> Result<()> {
        let prefix: Uri = prefix.as_ref().parse()?;

        let scope_id = self.scopes.add_node(self.scope_id, {
            let parent = &self.scopes[self.scope_id].data;
            ScopeData {
                prefix: parent.prefix.join(&prefix)?,
                default_handler: None,
            }
        })?;

        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id,
                modifier: &*self.modifier,
            })
            .map_err(Into::into)?;

        Ok(())
    }

    /// Applies the specified configuration with a `ModifyHandler` on the current scope.
    pub fn modify<M2>(
        &mut self,
        modifier: M2,
        config: impl Config<Chain<&'a M, M2>, T>,
    ) -> Result<()> {
        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id: self.scope_id,
                modifier: &Chain::new(self.modifier, modifier),
            })
            .map_err(Into::into)
    }
}

/// A trait representing a set of elements that will be registered into a certain scope.
pub trait Config<M, T: AppData> {
    type Error: Into<Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error>;
}

impl<F, M, T, E> Config<M, T> for F
where
    F: FnOnce(&mut Scope<'_, M, T>) -> std::result::Result<(), E>,
    E: Into<Error>,
    T: AppData,
{
    type Error = E;

    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        self(cx)
    }
}

impl<S1, S2, M, T> Config<M, T> for Chain<S1, S2>
where
    S1: Config<M, T>,
    S2: Config<M, T>,
    T: AppData,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

impl<M, S, T> Config<M, T> for Option<S>
where
    S: Config<M, T>,
    T: AppData,
{
    type Error = S::Error;

    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        if let Some(scope) = self {
            scope.configure(cx)?;
        }
        Ok(())
    }
}

impl<M, S, E, T> Config<M, T> for std::result::Result<S, E>
where
    S: Config<M, T>,
    E: Into<Error>,
    T: AppData,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        self.map_err(Into::into)?.configure(cx).map_err(Into::into)
    }
}

impl<M, T> Config<M, T> for ()
where
    T: AppData,
{
    type Error = Never;

    fn configure(self, _: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}
