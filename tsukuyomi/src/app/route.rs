use {
    super::{
        config::{AppConfig, AppConfigContext},
        uri::{Uri, UriComponent},
    },
    crate::{
        core::{Chain, Never, TryFrom, TryInto},
        extractor::Extractor,
        fs::NamedFile,
        future::{Future, MaybeFuture},
        generic::{Combine, Func},
        handler::{Handler, MakeHandler, ModifyHandler},
        input::param::{FromPercentEncoded, PercentEncoded},
        output::Responder,
    },
    http::{HttpTryFrom, Method, StatusCode},
    indexmap::{indexset, IndexSet},
    std::{marker::PhantomData, path::Path},
};

/// A set of request methods that a route accepts.
#[derive(Debug)]
pub struct Methods(pub(super) IndexSet<Method>);

impl TryFrom<Self> for Methods {
    type Error = Never;

    #[inline]
    fn try_from(methods: Self) -> Result<Self, Self::Error> {
        Ok(methods)
    }
}

impl TryFrom<Method> for Methods {
    type Error = Never;

    #[inline]
    fn try_from(method: Method) -> Result<Self, Self::Error> {
        Ok(Methods(indexset! { method }))
    }
}

impl<M> TryFrom<Vec<M>> for Methods
where
    Method: HttpTryFrom<M>,
{
    type Error = http::Error;

    #[inline]
    fn try_from(methods: Vec<M>) -> Result<Self, Self::Error> {
        let methods = methods
            .into_iter()
            .map(Method::try_from)
            .collect::<Result<_, _>>()
            .map_err(Into::into)?;
        Ok(Methods(methods))
    }
}

impl<'a> TryFrom<&'a str> for Methods {
    type Error = failure::Error;

    #[inline]
    fn try_from(methods: &'a str) -> Result<Self, Self::Error> {
        let methods = methods
            .split(',')
            .map(|s| Method::try_from(s.trim()).map_err(Into::into))
            .collect::<http::Result<_>>()?;
        Ok(Methods(methods))
    }
}

mod tags {
    #[derive(Debug)]
    pub struct Completed(());

    #[derive(Debug)]
    pub struct Incomplete(());
}

pub fn root() -> Builder<(), self::tags::Incomplete> {
    Builder {
        uri: Uri::root(),
        methods: None,
        extractor: (),
        _marker: std::marker::PhantomData,
    }
}

pub fn asterisk() -> Builder<(), self::tags::Completed> {
    Builder {
        uri: Uri::asterisk(),
        methods: Some(Methods(indexset! { Method::OPTIONS })),
        extractor: (),
        _marker: std::marker::PhantomData,
    }
}

/// A builder of `Scope` to register a route, which is matched to the requests
/// with a certain path and method(s) and will return its response.
#[derive(Debug)]
pub struct Builder<E: Extractor = (), T = self::tags::Incomplete> {
    uri: Uri,
    methods: Option<Methods>,
    extractor: E,
    _marker: PhantomData<T>,
}

impl<E> Builder<E, self::tags::Incomplete>
where
    E: Extractor,
{
    /// Sets the HTTP methods that this route accepts.
    pub fn methods(self, methods: impl TryInto<Methods>) -> super::Result<Self> {
        Ok(Builder {
            methods: Some(methods.try_into()?),
            ..self
        })
    }

    /// Appends a *static* segment into this route.
    pub fn segment(mut self, s: impl Into<String>) -> super::Result<Self> {
        self.uri.push(UriComponent::Static(s.into()))?;
        Ok(self)
    }

    /// Appends a trailing slash to the path of this route.
    pub fn slash(self) -> Builder<E, self::tags::Completed> {
        Builder {
            uri: {
                let mut uri = self.uri;
                uri.push(UriComponent::Slash).expect("this is a bug.");
                uri
            },
            methods: self.methods,
            extractor: self.extractor,
            _marker: PhantomData,
        }
    }

    /// Appends a parameter with the specified name to the path of this route.
    pub fn param<T>(
        self,
        name: impl Into<String>,
    ) -> super::Result<
        Builder<impl Extractor<Output = <E::Output as Combine<(T,)>>::Out>, self::tags::Incomplete>,
    >
    where
        T: FromPercentEncoded + Send + 'static,
        E::Output: Combine<(T,)> + Send + 'static,
    {
        let name = name.into();
        Ok(Builder {
            uri: {
                let mut uri = self.uri;
                uri.push(UriComponent::Param(name.clone(), ':'))?;
                uri
            },
            methods: self.methods,
            extractor: Chain::new(
                self.extractor,
                crate::extractor::ready(move |input| match input.params {
                    Some(ref params) => {
                        let s = params.name(&name).ok_or_else(|| {
                            crate::error::internal_server_error("invalid paramter name")
                        })?;
                        T::from_percent_encoded(unsafe { PercentEncoded::new_unchecked(s) })
                            .map_err(Into::into)
                    }
                    None => Err(crate::error::internal_server_error("missing Params")),
                }),
            ),
            _marker: PhantomData,
        })
    }

    /// Appends a *catch-all* parameter with the specified name to the path of this route.
    pub fn catch_all<T>(
        self,
        name: impl Into<String>,
    ) -> super::Result<
        Builder<impl Extractor<Output = <E::Output as Combine<(T,)>>::Out>, self::tags::Completed>,
    >
    where
        T: FromPercentEncoded + Send + 'static,
        E::Output: Combine<(T,)> + Send + 'static,
    {
        let name = name.into();
        Ok(Builder {
            uri: {
                let mut uri = self.uri;
                uri.push(UriComponent::Param(name.clone(), '*'))?;
                uri
            },
            methods: self.methods,
            extractor: Chain::new(
                self.extractor,
                crate::extractor::ready(|input| match input.params {
                    Some(ref params) => {
                        let s = params.catch_all().ok_or_else(|| {
                            crate::error::internal_server_error(
                                "the catch-all parameter is not available",
                            )
                        })?;
                        T::from_percent_encoded(unsafe { PercentEncoded::new_unchecked(s) })
                            .map_err(Into::into)
                    }
                    None => Err(crate::error::internal_server_error("missing Params")),
                }),
            ),
            _marker: PhantomData,
        })
    }
}

impl<E, T> Builder<E, T>
where
    E: Extractor,
{
    /// Appends a supplemental `Extractor` to this route.
    pub fn extract<E2>(self, other: E2) -> Builder<Chain<E, E2>, T>
    where
        E2: Extractor,
        E::Output: Combine<E2::Output> + Send + 'static,
        E2::Output: Send + 'static,
    {
        Builder {
            extractor: Chain::new(self.extractor, other),
            uri: self.uri,
            methods: self.methods,
            _marker: PhantomData,
        }
    }

    /// Finalize the configuration in this route and creates the instance of `Route`.
    pub fn finish<F>(self, make_handler: F) -> Route<impl Handler<Output = F::Output>>
    where
        F: MakeHandler<E>,
    {
        let allowed_methods = self
            .methods
            .unwrap_or_else(|| Methods(indexset! { Method::GET }));

        let handler = {
            let inner = make_handler.make_handler(self.extractor);
            let allowed_methods = allowed_methods.0.clone();
            crate::handler::raw(move |input| {
                if !allowed_methods.contains(input.request.method()) {
                    return MaybeFuture::err(StatusCode::METHOD_NOT_ALLOWED.into());
                }
                inner.call(input).map_err(Into::into)
            })
        };

        Route {
            uri: self.uri,
            allowed_methods,
            handler,
        }
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The provided function always succeeds and immediately returns a value.
    pub fn reply<F>(self, f: F) -> Route<impl Handler<Output = F::Out>>
    where
        F: Func<E::Output> + Clone + Send + 'static,
    {
        self.finish(|extractor: E| {
            crate::handler::raw(move |input| match extractor.extract(input) {
                MaybeFuture::Ready(result) => {
                    MaybeFuture::Ready(result.map(|args| f.call(args)).map_err(Into::into))
                }
                MaybeFuture::Future(mut future) => MaybeFuture::Future({
                    let f = f.clone();
                    crate::future::poll_fn(move |cx| {
                        let args = futures01::try_ready!(future.poll_ready(cx).map_err(Into::into));
                        Ok(f.call(args).into())
                    })
                }),
            })
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The result of provided function is returned by `Future`.
    pub fn call<F, R>(self, f: F) -> Route<impl Handler<Output = R::Output>>
    where
        F: Func<E::Output, Out = R> + Clone + Send + 'static,
        R: Future + Send + 'static,
    {
        #[allow(missing_debug_implementations)]
        enum State<F1, F2, F> {
            First(F1, F),
            Second(F2),
        }

        self.finish(|extractor: E| {
            crate::handler::raw(move |input| {
                let mut state = match extractor.extract(input) {
                    MaybeFuture::Ready(Ok(args)) => State::Second(f.call(args)),
                    MaybeFuture::Ready(Err(err)) => return MaybeFuture::err(err.into()),
                    MaybeFuture::Future(future) => State::First(future, f.clone()),
                };
                MaybeFuture::Future(crate::future::poll_fn(move |cx| loop {
                    state = match state {
                        State::First(ref mut f1, ref f) => {
                            let args = futures01::try_ready!(f1.poll_ready(cx).map_err(Into::into));
                            State::Second(f.call(args))
                        }
                        State::Second(ref mut f2) => return f2.poll_ready(cx).map_err(Into::into),
                    }
                }))
            })
        })
    }
}

impl<T> Builder<(), T> {
    /// Builds a `Route` that uses the specified `Handler` directly.
    pub fn raw<H>(self, handler: H) -> Route<impl Handler<Output = H::Output>>
    where
        H: Handler,
    {
        self.finish(|_: ()| handler)
    }
}

impl<E, T> Builder<E, T>
where
    E: Extractor<Output = ()>,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<R>(self, output: R) -> Route<impl Handler<Output = R>>
    where
        R: Clone + Send + 'static,
    {
        self.reply(move || output.clone())
    }

    /// Creates a `Route` that sends the contents of file located at the specified path.
    pub fn send_file(
        self,
        path: impl AsRef<Path>,
        config: Option<crate::fs::OpenConfig>,
    ) -> Route<impl Handler<Output = NamedFile>> {
        let path = crate::fs::ArcPath::from(path.as_ref().to_path_buf());

        self.call(move || {
            crate::future::Compat01::from(match config {
                Some(ref config) => NamedFile::open_with_config(path.clone(), config.clone()),
                None => NamedFile::open(path.clone()),
            })
        })
    }
}

#[derive(Debug)]
pub struct Route<H> {
    uri: Uri,
    allowed_methods: Methods,
    handler: H,
}

impl<H, M> AppConfig<M> for Route<H>
where
    H: Handler,
    M: ModifyHandler<H>,
    M::Output: Responder,
    M::Handler: Send + Sync + 'static,
{
    type Error = super::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        cx.add_route(self.uri, self.allowed_methods, self.handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_methods_try_from() {
        assert_eq!(
            Methods::try_from(Methods(indexset! { Method::GET }))
                .unwrap()
                .0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from(Method::GET).unwrap().0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from(vec![Method::GET, Method::POST])
                .unwrap()
                .0,
            indexset! { Method::GET, Method::POST }
        );
        assert_eq!(
            Methods::try_from("GET").unwrap().0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from("GET, POST").unwrap().0,
            indexset! { Method::GET , Method::POST }
        );
        assert!(Methods::try_from("").is_err());
    }
}
