use {
    super::{
        super::uri::{Uri, UriComponent},
        Route,
    },
    crate::{
        endpoint::Endpoint,
        extractor::Extractor,
        future::{Poll, TryFuture},
        generic::{Combine, Tuple},
        handler::Handler,
        input::{
            param::{FromPercentEncoded, PercentEncoded},
            Input,
        },
        util::Chain,
    },
    std::{marker::PhantomData, sync::Arc},
};

mod tags {
    #[derive(Debug)]
    pub struct Completed(());

    #[derive(Debug)]
    pub struct Incomplete(());
}

#[derive(Debug)]
pub struct Context<'a> {
    components: Vec<UriComponent>,
    _marker: PhantomData<&'a mut ()>,
}

pub trait PathConfig {
    type Output: Tuple;
    type Extractor: Extractor<Output = Self::Output>;
    type Tag;

    fn configure(self, cx: &mut Context<'_>) -> super::Result<Self::Extractor>;
}

impl<L, R> PathConfig for Chain<L, R>
where
    L: PathConfig<Tag = self::tags::Incomplete>,
    R: PathConfig,
    L::Output: Combine<R::Output>,
{
    type Output = <L::Output as Combine<R::Output>>::Out;
    type Extractor = Chain<L::Extractor, R::Extractor>;
    type Tag = R::Tag;

    fn configure(self, cx: &mut Context<'_>) -> super::Result<Self::Extractor> {
        let left = self.left.configure(cx)?;
        let right = self.right.configure(cx)?;
        Ok(Chain::new(left, right))
    }
}

impl PathConfig for String {
    type Output = ();
    type Extractor = ();
    type Tag = self::tags::Incomplete;

    fn configure(self, cx: &mut Context<'_>) -> super::Result<Self::Extractor> {
        // TODO: validatation
        cx.components.push(UriComponent::Static(self));
        Ok(())
    }
}

impl<'a> PathConfig for &'a str {
    type Output = ();
    type Extractor = ();
    type Tag = self::tags::Incomplete;

    fn configure(self, cx: &mut Context<'_>) -> super::Result<Self::Extractor> {
        // TODO: validatation
        cx.components.push(UriComponent::Static(self.to_owned()));
        Ok(())
    }
}

/// Creates a `PathConfig` that appends a trailing slash to the path.
pub fn slash() -> Slash {
    Slash(())
}

#[derive(Debug)]
pub struct Slash(());

impl PathConfig for Slash {
    type Output = ();
    type Extractor = ();
    type Tag = self::tags::Completed;

    fn configure(self, cx: &mut Context<'_>) -> super::Result<Self::Extractor> {
        cx.components.push(UriComponent::Slash);
        Ok(())
    }
}

/// Creates a `PathConfig` that appends a parameter with the specified name to the path.
pub fn param<T>(name: impl Into<String>) -> Param<T>
where
    T: FromPercentEncoded,
{
    Param {
        name: name.into(),
        _marker: PhantomData,
    }
}

#[derive(Debug)]
pub struct Param<T> {
    name: String,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Param<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T> PathConfig for Param<T>
where
    T: FromPercentEncoded,
{
    type Output = (T,);
    type Extractor = Self;
    type Tag = self::tags::Incomplete;

    fn configure(self, cx: &mut Context<'_>) -> super::Result<Self::Extractor> {
        // TODO: validatation
        cx.components
            .push(UriComponent::Param(self.name.clone(), ':'));
        Ok(self)
    }
}

impl<T> Extractor for Param<T>
where
    T: FromPercentEncoded,
{
    type Output = (T,);
    type Error = crate::error::Error;
    type Extract = Self;

    fn extract(&self) -> Self::Extract {
        self.clone()
    }
}

impl<T> TryFuture for Param<T>
where
    T: FromPercentEncoded,
{
    type Ok = (T,);
    type Error = crate::Error;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<(T,), crate::Error> {
        let params = input
            .params
            .as_ref()
            .ok_or_else(|| crate::error::internal_server_error("missing Params"))?;
        let s = params
            .name(&self.name)
            .ok_or_else(|| crate::error::internal_server_error("invalid paramter name"))?;
        T::from_percent_encoded(unsafe { PercentEncoded::new_unchecked(s) })
            .map(|x| (x,).into())
            .map_err(Into::into)
    }
}

/// Creates a `PathConfig` that appends a *catch-all* parameter with the specified name to the path.
pub fn catch_all<T>(name: impl Into<String>) -> CatchAll<T>
where
    T: FromPercentEncoded,
{
    CatchAll {
        name: name.into(),
        _marker: PhantomData,
    }
}

#[derive(Debug)]
pub struct CatchAll<T> {
    name: String,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Clone for CatchAll<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T> PathConfig for CatchAll<T>
where
    T: FromPercentEncoded,
{
    type Output = (T,);
    type Extractor = Self;
    type Tag = self::tags::Completed;

    fn configure(self, cx: &mut Context<'_>) -> super::Result<Self::Extractor> {
        // TODO: validatation
        cx.components
            .push(UriComponent::Param(self.name.clone(), '*'));
        Ok(self)
    }
}

impl<T> Extractor for CatchAll<T>
where
    T: FromPercentEncoded,
{
    type Output = (T,);
    type Error = crate::error::Error;
    type Extract = Self;

    fn extract(&self) -> Self::Extract {
        self.clone()
    }
}

impl<T> TryFuture for CatchAll<T>
where
    T: FromPercentEncoded,
{
    type Ok = (T,);
    type Error = crate::Error;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<(T,), crate::Error> {
        let params = input
            .params
            .as_ref()
            .ok_or_else(|| crate::error::internal_server_error("missing Params"))?;
        let s = params
            .catch_all()
            .ok_or_else(|| crate::error::internal_server_error("invalid paramter name"))?;
        T::from_percent_encoded(unsafe { PercentEncoded::new_unchecked(s) })
            .map(|x| (x,).into())
            .map_err(Into::into)
    }
}

/// A macro for generating the code that creates a [`Path`] from the provided tokens.
///
/// [`Path`]: ./app/config/route/struct.Path.html
#[macro_export]
macro_rules! path {
    (/) => ( $crate::config::path::Path::root() );
    (*) => ( $crate::config::path::Path::asterisk() );
    ($(/ $s:tt)+) => ( $crate::config::path::Path::create($crate::chain!($($s),*)).unwrap() );
    ($(/ $s:tt)+ /) => ( $crate::config::route::Path::create($crate::chain!($($s),*, $crate::app::config::route::slash())).unwrap() );
}

#[derive(Debug)]
pub struct Path<E: Extractor = ()> {
    uri: Option<Uri>,
    extractor: E,
}

impl Path<()> {
    pub fn root() -> Self {
        Self {
            uri: Some(Uri::root()),
            extractor: (),
        }
    }

    pub fn asterisk() -> Self {
        Self {
            uri: None,
            extractor: (),
        }
    }

    pub fn create<T>(config: T) -> super::Result<Path<T::Extractor>>
    where
        T: PathConfig,
    {
        let mut cx = Context {
            components: vec![],
            _marker: PhantomData,
        };
        let extractor = config.configure(&mut cx)?;

        let mut uri = Uri::root();
        for component in cx.components {
            uri.push(component)?;
        }

        Ok(Path {
            uri: Some(uri),
            extractor,
        })
    }
}

impl<E> Path<E>
where
    E: Extractor,
{
    /// Finalize the configuration in this route and creates the instance of `Route`.
    pub fn to<T>(
        self,
        endpoint: T,
    ) -> Route<
        impl Handler<
            Output = T::Output,
            Handle = self::handler::RouteHandle<E, T>, // private
        >,
    >
    where
        T: Endpoint<E::Output>,
    {
        let Self { uri, extractor, .. } = self;

        let allowed_methods = endpoint.allowed_methods();
        let extractor = Arc::new(extractor);
        let endpoint = Arc::new(endpoint);

        Route {
            uri,
            handler: crate::handler::handler(
                move || self::handler::RouteHandle {
                    extractor: extractor.clone(),
                    endpoint: endpoint.clone(),
                    state: self::handler::RouteHandleState::Init,
                },
                allowed_methods,
            ),
        }
    }
}

mod handler {
    use {
        crate::{
            endpoint::{ApplyContext, Endpoint, EndpointAction},
            error::Error,
            extractor::Extractor,
            future::TryFuture,
            input::Input,
        },
        futures01::{try_ready, Poll},
        std::sync::Arc,
    };

    #[allow(missing_debug_implementations)]
    pub struct RouteHandle<E, T>
    where
        E: Extractor,
        T: Endpoint<E::Output>,
    {
        pub(super) extractor: Arc<E>,
        pub(super) endpoint: Arc<T>,
        pub(super) state: RouteHandleState<E, T>,
    }

    #[allow(missing_debug_implementations)]
    pub(super) enum RouteHandleState<E, T>
    where
        E: Extractor,
        T: Endpoint<E::Output>,
    {
        Init,
        First(E::Extract, Option<T::Action>),
        Second(<T::Action as EndpointAction<E::Output>>::Future),
    }

    impl<E, T> TryFuture for RouteHandle<E, T>
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
                    RouteHandleState::Init => {
                        let action = self.endpoint.apply(&mut ApplyContext::new(input))?;
                        let extract = self.extractor.extract();
                        RouteHandleState::First(extract, Some(action))
                    }
                    RouteHandleState::First(ref mut future, ref mut action) => {
                        let args = try_ready!(future.poll_ready(input).map_err(Into::into));
                        let future = action.take().unwrap().invoke(args);
                        RouteHandleState::Second(future)
                    }
                    RouteHandleState::Second(ref mut action) => {
                        return action.poll_ready(input).map_err(Into::into)
                    }
                }
            }
        }
    }
}
