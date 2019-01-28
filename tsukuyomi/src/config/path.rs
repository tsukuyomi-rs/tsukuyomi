use {
    super::Route,
    crate::{
        endpoint::Endpoint, //
        error::Error,
        generic::Tuple,
        handler::{metadata::Metadata, Handler},
        input::param::Params,
    },
    std::{marker::PhantomData, sync::Arc},
};

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

/// A macro for generating the code that creates a [`Path`] from the provided tokens.
///
/// [`Path`]: ./app/config/route/struct.Path.html
#[macro_export]
macro_rules! path {
    ($path:expr) => {{
        use $crate::config::path::internal as __path_internal;
        enum __Dummy {}
        impl __Dummy {
            $crate::config::path::path_impl!(__path_internal, $path);
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
        let Self { path, .. } = self;
        let endpoint = Arc::new(endpoint);

        let mut metadata = match path {
            "*" => Metadata::without_suffix(),
            path => Metadata::new(path.parse().expect("this is a bug")),
        };
        *metadata.allowed_methods_mut() = endpoint.allowed_methods();

        Route {
            handler: crate::handler::handler(
                move || self::handle::RouteHandle::new(endpoint.clone()),
                metadata,
            ),
        }
    }
}

mod handle {
    use {
        super::PathExtractor,
        crate::{
            endpoint::{ApplyContext, Endpoint},
            error::Error,
            future::{Poll, TryFuture},
            input::Input,
        },
        std::{marker::PhantomData, sync::Arc},
    };

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

    impl<E, T> RouteHandle<E, T>
    where
        E: PathExtractor,
        T: Endpoint<E::Output>,
    {
        pub fn new(endpoint: Arc<T>) -> Self {
            Self {
                state: RouteHandleState::Init(endpoint),
                _marker: PhantomData,
            }
        }
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
