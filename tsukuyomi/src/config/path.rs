use {
    super::Route,
    crate::{
        endpoint::Endpoint,
        error::Error,
        generic::{Combine, Tuple},
        handler::Handler,
        input::param::{FromPercentEncoded, Params, PercentEncoded},
        util::Chain,
    },
    failure::format_err,
    std::{borrow::Cow, collections::HashSet, marker::PhantomData, sync::Arc},
};

mod tags {
    #[derive(Debug)]
    pub struct Completed(());

    #[derive(Debug)]
    pub struct Incomplete(());
}

pub trait PathExtractor: Clone {
    type Output: Tuple;
    fn extract(&self, params: Option<&Params<'_>>) -> Result<Self::Output, Error>;
}

impl PathExtractor for () {
    type Output = ();

    #[inline]
    fn extract(&self, _: Option<&Params<'_>>) -> Result<Self::Output, Error> {
        Ok(())
    }
}

impl<L, R> PathExtractor for Chain<L, R>
where
    L: PathExtractor,
    R: PathExtractor,
    L::Output: Combine<R::Output>,
{
    type Output = <L::Output as Combine<R::Output>>::Out;

    fn extract(&self, params: Option<&Params<'_>>) -> Result<Self::Output, Error> {
        let left = self.left.extract(params)?;
        let right = self.right.extract(params)?;
        Ok(left.combine(right))
    }
}

// ==== PathConfig ====

#[derive(Debug)]
pub struct Context<'a> {
    path: &'a mut String,
    names: &'a mut HashSet<&'static str>,
}

pub trait PathConfig {
    type Output: Tuple;
    type Extractor: PathExtractor<Output = Self::Output>;
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

impl PathConfig for &'static str {
    type Output = ();
    type Extractor = ();
    type Tag = self::tags::Incomplete;

    fn configure(self, cx: &mut Context<'_>) -> super::Result<Self::Extractor> {
        if self.is_empty() {
            return Err(format_err!("path segment cannot be empty").into());
        }

        if !self.is_ascii() {
            return Err(format_err!("path segment must be an ASCII sequence").into());
        }

        if self.contains('/') {
            return Err(format_err!("path segment cannot contain slash").into());
        }

        cx.path.push('/');
        cx.path.push_str(self); // FIXME: percent-encode

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
        if !cx.path.ends_with('/') {
            cx.path.push('/');
        }
        Ok(())
    }
}

/// Creates a `PathConfig` that appends a parameter with the specified name to the path.
pub fn param<T>(name: &'static str) -> Param<T>
where
    T: FromPercentEncoded,
{
    Param {
        name,
        _marker: PhantomData,
    }
}

#[derive(Debug)]
pub struct Param<T> {
    name: &'static str,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for Param<T> {}

impl<T> Clone for Param<T> {
    fn clone(&self) -> Self {
        *self
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
        if self.name.is_empty() {
            return Err(format_err!("parameter name cannot be empty").into());
        }

        if !self.name.is_ascii() {
            return Err(format_err!("parameter name must be an ASCII sequence").into());
        }

        if !cx.names.insert(self.name) {
            return Err(format_err!("duplicated parameter name: '{}'", self.name).into());
        }

        cx.path.push('/');
        cx.path.push(':');
        cx.path.push_str(self.name);

        Ok(self)
    }
}

impl<T> PathExtractor for Param<T>
where
    T: FromPercentEncoded,
{
    type Output = (T,);

    fn extract(&self, params: Option<&Params<'_>>) -> Result<Self::Output, Error> {
        let s = params
            .ok_or_else(|| crate::error::internal_server_error("missing Params"))?
            .name(&self.name)
            .ok_or_else(|| crate::error::internal_server_error("invalid paramter name"))?;
        T::from_percent_encoded(unsafe { PercentEncoded::new_unchecked(s) })
            .map(|x| (x,))
            .map_err(Into::into)
    }
}

/// Creates a `PathConfig` that appends a *catch-all* parameter with the specified name to the path.
pub fn catch_all<T>(name: &'static str) -> CatchAll<T>
where
    T: FromPercentEncoded,
{
    CatchAll {
        name,
        _marker: PhantomData,
    }
}

#[derive(Debug)]
pub struct CatchAll<T> {
    name: &'static str,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for CatchAll<T> {}

impl<T> Clone for CatchAll<T> {
    fn clone(&self) -> Self {
        *self
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
        if self.name.is_empty() {
            return Err(format_err!("parameter name cannot be empty").into());
        }

        if !self.name.is_ascii() {
            return Err(format_err!("parameter name must be an ASCII sequence").into());
        }

        if !cx.names.insert(self.name) {
            return Err(format_err!("duplicated parameter name: '{}'", self.name).into());
        }

        cx.path.push('/');
        cx.path.push('*');
        cx.path.push_str(self.name);

        Ok(self)
    }
}

impl<T> PathExtractor for CatchAll<T>
where
    T: FromPercentEncoded,
{
    type Output = (T,);

    fn extract(&self, params: Option<&Params<'_>>) -> Result<Self::Output, Error> {
        let s = params
            .ok_or_else(|| crate::error::internal_server_error("missing Params"))?
            .catch_all()
            .ok_or_else(|| crate::error::internal_server_error("invalid paramter name"))?;
        T::from_percent_encoded(unsafe { PercentEncoded::new_unchecked(s) })
            .map(|x| (x,))
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
    ($path:expr) => ( compile_error!("the procedural macro has not been implemented yet.") );
}

#[derive(Debug)]
pub struct Path<E: PathExtractor = ()> {
    path: Cow<'static, str>,
    extractor: E,
}

impl Path<()> {
    pub fn root() -> Self {
        Self {
            path: "/".into(),
            extractor: (),
        }
    }

    pub fn asterisk() -> Self {
        Self {
            path: "*".into(),
            extractor: (),
        }
    }

    pub fn create<T>(config: T) -> super::Result<Path<T::Extractor>>
    where
        T: PathConfig,
    {
        let mut path = String::new();
        let mut names = HashSet::new();
        let extractor = config.configure(&mut Context {
            path: &mut path,
            names: &mut names,
        })?;

        Ok(Path {
            path: if path.is_empty() {
                "/".into()
            } else {
                path.into()
            },
            extractor,
        })
    }
}

impl<E> Path<E>
where
    E: PathExtractor,
{
    #[doc(hidden)]
    pub fn new(path: impl Into<Cow<'static, str>>, extractor: E) -> Self {
        Self {
            path: path.into(),
            extractor,
        }
    }

    /// Finalize the configuration in this route and creates the instance of `Route`.
    pub fn to<T>(
        self,
        endpoint: T,
    ) -> Route<
        impl Handler<
            Output = T::Output,
            Error = Error,
            Handle = self::handler::RouteHandle<E, T>, // private
        >,
    >
    where
        T: Endpoint<E::Output>,
    {
        let Self {
            path, extractor, ..
        } = self;
        let endpoint = Arc::new(endpoint);
        let allowed_methods = endpoint.allowed_methods();

        Route {
            path,
            handler: crate::handler::handler(
                move || self::handler::RouteHandle {
                    extractor: extractor.clone(),
                    endpoint: endpoint.clone(),
                    in_flight: None,
                },
                allowed_methods,
            ),
        }
    }
}

mod handler {
    use {
        super::PathExtractor,
        crate::{
            endpoint::{ApplyContext, Endpoint},
            error::Error,
            future::{Poll, TryFuture},
            input::Input,
        },
        std::sync::Arc,
    };

    #[allow(missing_debug_implementations)]
    pub struct RouteHandle<E, T>
    where
        E: PathExtractor,
        T: Endpoint<E::Output>,
    {
        pub(super) extractor: E,
        pub(super) endpoint: Arc<T>,
        pub(super) in_flight: Option<T::Future>,
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
                if let Some(ref mut action) = self.in_flight {
                    return action.poll_ready(input).map_err(Into::into);
                }

                let args = self.extractor.extract(input.params.as_ref())?;
                self.in_flight = Some(
                    self.endpoint
                        .apply(args, &mut ApplyContext::new(input))
                        .map_err(|(_args, err)| err)?,
                );
            }
        }
    }
}
