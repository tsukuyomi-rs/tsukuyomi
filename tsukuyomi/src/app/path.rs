use {
    crate::{
        endpoint::Endpoint, //
        error::Error,
        generic::Tuple,
        handler::{metadata::Metadata, Handler},
        input::param::Params,
    },
    std::{marker::PhantomData, sync::Arc},
};

#[allow(deprecated)]
use crate::config::Route;

#[doc(hidden)]
pub use tsukuyomi_macros::path_impl;

pub trait PathExtractor {
    type Output: Tuple;

    fn extract(params: Option<&Params<'_>>) -> Result<Self::Output, Error>;
}

impl PathExtractor for () {
    type Output = ();

    #[inline]
    fn extract(_: Option<&Params<'_>>) -> Result<Self::Output, Error> {
        Ok(())
    }
}

pub trait IntoPath {
    type Output: Tuple;
    type Extractor: PathExtractor<Output = Self::Output>;

    fn into_path(self) -> Path<Self::Extractor>;
}

impl IntoPath for &'static str {
    type Output = ();
    type Extractor = ();

    fn into_path(self) -> Path<Self::Extractor> {
        Path::new(self)
    }
}

impl<T> IntoPath for Path<T>
where
    T: PathExtractor,
{
    type Output = T::Output;
    type Extractor = T;

    fn into_path(self) -> Path<Self::Extractor> {
        self
    }
}

/// A macro for generating the code that creates a [`Path`] from the provided tokens.
///
/// [`Path`]: ./app/config/route/struct.Path.html
#[macro_export]
macro_rules! path {
    ($path:expr) => {{
        use $crate::app::path::internal as __path_internal;
        enum __Dummy {}
        impl __Dummy {
            $crate::app::path::path_impl!(__path_internal, $path);
        }
        __Dummy::call()
    }};
}

#[doc(hidden)]
pub mod internal {
    pub use {
        super::{Path, PathExtractor},
        crate::{
            error::Error,
            input::param::{FromPercentEncoded, Params, PercentEncoded},
        },
    };
}

#[derive(Debug)]
pub struct Path<E: PathExtractor = ()> {
    path: &'static str,
    _marker: PhantomData<E>,
}

impl<E> Path<E>
where
    E: PathExtractor,
{
    /// Creates a new `Path` with the specified path and extractor.
    pub fn new(path: &'static str) -> Self {
        Self {
            path,
            _marker: PhantomData,
        }
    }

    /// Creates a `Route` with this path configuration and the specified `Endpoint`.
    #[deprecated]
    #[allow(deprecated)]
    pub fn to<T>(
        self,
        endpoint: T,
    ) -> Route<
        impl Handler<
            Output = T::Output,
            Error = Error,
            Handle = self::handle::RouteHandle<E, T>, // private
        >,
    >
    where
        T: Endpoint<E::Output>,
    {
        Route {
            handler: RouteHandler::new(self, endpoint),
        }
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
        let Path { path, .. } = path;
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
