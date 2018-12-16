use {
    super::{
        recognizer::Recognizer,
        scope::{ScopeId, Scopes},
        App, AppInner, Endpoint, EndpointId, ScopeData, Uri,
    },
    crate::{
        core::{Chain, Never},
        handler::{Handler, ModifyHandler},
        output::Responder,
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
pub(super) fn configure(config: impl Config<()>) -> Result<App> {
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
pub struct Scope<'a, M> {
    recognizer: &'a mut Recognizer<Endpoint>,
    scopes: &'a mut Scopes<ScopeData>,
    modifier: &'a M,
    scope_id: ScopeId,
}

impl<'a, M> Scope<'a, M> {
    /// Appends a `Handler` with the specified URI onto the current scope.
    pub fn route<H>(&mut self, uri: Option<impl AsRef<str>>, handler: H) -> Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Output: Responder,
        <M::Output as Responder>::Future: Send + 'static,
        M::Handler: Send + Sync + 'static,
        <M::Handler as Handler>::Handle: Send + 'static,
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
                    handler: Box::new(self.modifier.modify(handler)),
                },
            )?;
        } else {
            self.scopes[self.scope_id].data.default_handler =
                Some(Box::new(self.modifier.modify(handler)));
        }
        Ok(())
    }

    /// Creates a sub-scope with the provided prefix onto the current scope.
    pub fn mount(&mut self, prefix: impl AsRef<str>, config: impl Config<M>) -> Result<()> {
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
        config: impl Config<Chain<&'a M, M2>>,
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
pub trait Config<M> {
    type Error: Into<Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut Scope<'_, M>) -> std::result::Result<(), Self::Error>;
}

impl<F, M, E> Config<M> for F
where
    F: FnOnce(&mut Scope<'_, M>) -> std::result::Result<(), E>,
    E: Into<Error>,
{
    type Error = E;

    fn configure(self, cx: &mut Scope<'_, M>) -> std::result::Result<(), Self::Error> {
        self(cx)
    }
}

impl<S1, S2, M> Config<M> for Chain<S1, S2>
where
    S1: Config<M>,
    S2: Config<M>,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M>) -> std::result::Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

impl<M, S> Config<M> for Option<S>
where
    S: Config<M>,
{
    type Error = S::Error;

    fn configure(self, cx: &mut Scope<'_, M>) -> std::result::Result<(), Self::Error> {
        if let Some(scope) = self {
            scope.configure(cx)?;
        }
        Ok(())
    }
}

impl<M, S, E> Config<M> for std::result::Result<S, E>
where
    S: Config<M>,
    E: Into<Error>,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M>) -> std::result::Result<(), Self::Error> {
        self.map_err(Into::into)?.configure(cx).map_err(Into::into)
    }
}

impl<M> Config<M> for () {
    type Error = Never;

    fn configure(self, _: &mut Scope<'_, M>) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}
