use {
    super::{
        super::uri::{Uri, UriComponent},
        AppConfig, AppConfigContext,
    },
    crate::{
        core::{Chain, TryInto},
        endpoint::{Endpoint, EndpointAction},
        extractor::Extractor,
        generic::Combine,
        handler::{Handler, ModifyHandler},
        input::param::{FromPercentEncoded, PercentEncoded},
        output::Responder,
    },
    futures01::Future,
    http::StatusCode,
    std::marker::PhantomData,
};

pub fn route() -> Builder<(), self::tags::Incomplete> {
    Route::builder()
}

#[derive(Debug)]
pub struct Route<H> {
    uri: Uri,
    handler: H,
}

impl Route<()> {
    pub fn builder() -> Builder<(), self::tags::Incomplete> {
        Builder {
            uri: Uri::root(),
            extractor: (),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<H> Route<H>
where
    H: Handler,
{
    pub fn from_parts(uri: impl TryInto<Uri>, handler: H) -> crate::app::Result<Self> {
        Ok(Self {
            uri: uri.try_into()?,
            handler,
        })
    }
}

impl<H, M> AppConfig<M> for Route<H>
where
    H: Handler,
    M: ModifyHandler<H>,
    M::Output: Responder,
    M::Handler: Send + Sync + 'static,
{
    type Error = crate::app::Error;

    fn configure(self, cx: &mut AppConfigContext<'_, M>) -> Result<(), Self::Error> {
        cx.add_route(self.uri, self.handler)
    }
}

mod tags {
    #[derive(Debug)]
    pub struct Completed(());

    #[derive(Debug)]
    pub struct Incomplete(());
}

/// A builder of `Scope` to register a route, which is matched to the requests
/// with a certain path and method(s) and will return its response.
#[derive(Debug)]
pub struct Builder<E: Extractor = (), T = self::tags::Incomplete> {
    uri: Uri,
    extractor: E,
    _marker: PhantomData<T>,
}

impl<E> Builder<E, self::tags::Incomplete>
where
    E: Extractor,
{
    /// Appends a *static* segment into this route.
    pub fn segment(mut self, s: impl Into<String>) -> crate::app::Result<Self> {
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
            extractor: self.extractor,
            _marker: PhantomData,
        }
    }

    /// Appends a parameter with the specified name to the path of this route.
    pub fn param<T>(
        self,
        name: impl Into<String>,
    ) -> crate::app::Result<
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
    ) -> crate::app::Result<
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

impl<E, Tag> Builder<E, Tag>
where
    E: Extractor,
{
    /// Appends a supplemental `Extractor` to this route.
    pub fn extract<E2>(self, other: E2) -> Builder<Chain<E, E2>, Tag>
    where
        E2: Extractor,
        E::Output: Combine<E2::Output> + Send + 'static,
        E2::Output: Send + 'static,
    {
        Builder {
            extractor: Chain::new(self.extractor, other),
            uri: self.uri,
            _marker: PhantomData,
        }
    }

    /// Finalize the configuration in this route and creates the instance of `Route`.
    pub fn to<T>(self, endpoint: T) -> Route<impl Handler<Output = T::Output>>
    where
        T: Endpoint<E::Output>,
        T::Action: Send + 'static,
        T::Output: 'static,
    {
        let Self { uri, extractor, .. } = self;
        let allowed_methods = endpoint.allowed_methods();

        let handler = crate::handler::handler(
            move |input| {
                #[allow(missing_debug_implementations)]
                enum State<F1, F2, T> {
                    Err(Option<crate::error::Error>),
                    First(F1, T),
                    Second(F2),
                }

                let mut state = {
                    match endpoint.apply(input.request.method()) {
                        Some(endpoint) => State::First(extractor.extract(input), Some(endpoint)),
                        None => State::Err(Some(StatusCode::METHOD_NOT_ALLOWED.into())),
                    }
                };

                crate::handler::handle(move |input| loop {
                    state = match state {
                        State::Err(ref mut err) => {
                            return Err(err.take().expect("the future has already polled"))
                        }
                        State::First(ref mut future, ref mut action) => {
                            let args = futures01::try_ready!(future.poll().map_err(Into::into));
                            let future = action.take().unwrap().call(input, args);
                            State::Second(future)
                        }
                        State::Second(ref mut future) => return future.poll().map_err(Into::into),
                    }
                })
            },
            allowed_methods,
        );

        Route { uri, handler }
    }
}
