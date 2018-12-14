use {
    crate::{
        core::{Chain, TryInto},
        endpoint::Endpoint,
        error::Error,
        extractor::Extractor,
        generic::{Combine, Func, Tuple},
        handler::AllowedMethods,
    },
    futures01::{Future, IntoFuture},
    http::Method,
};

pub fn any() -> Builder {
    Builder::allow_any()
}

pub fn allow_only(methods: impl TryInto<AllowedMethods>) -> crate::app::Result<Builder> {
    Builder::allow_only(methods)
}

macro_rules! define_builder_with_allowing_sigle_method {
    ($(
        $(#[$m:meta])*
        $name:ident => $METHOD:ident,
    )*) => {$(
        $(#[$m])*
        pub fn $name() -> Builder {
            Builder {
                extractor: (),
                allowed_methods: Some(Method::$METHOD.into()),
            }
        }
    )*}
}

define_builder_with_allowing_sigle_method! {
    get => GET,
    post => POST,
    put => PUT,
    delete => DELETE,
    head => HEAD,
    options => OPTIONS,
    connect => CONNECT,
    patch => PATCH,
    trace => TRACE,
}

pub fn get_or_head() -> Builder {
    Builder::allow_only(vec![Method::GET, Method::HEAD]).expect("should be valid methods")
}

#[derive(Debug)]
pub struct Builder<E: Extractor = ()> {
    extractor: E,
    allowed_methods: Option<AllowedMethods>,
}

impl Builder {
    pub fn allow_any() -> Self {
        Self {
            extractor: (),
            allowed_methods: None,
        }
    }

    pub fn allow_only(methods: impl TryInto<AllowedMethods>) -> crate::app::Result<Self> {
        Ok(Self {
            extractor: (),
            allowed_methods: Some(methods.try_into()?),
        })
    }
}

impl<E> Builder<E>
where
    E: Extractor + Send + Sync + 'static,
{
    /// Appends a supplemental `Extractor` to this route.
    pub fn extract<E2>(self, other: E2) -> Builder<Chain<E, E2>>
    where
        E2: Extractor,
        E::Output: Combine<E2::Output> + Send + 'static,
        E2::Output: Send + 'static,
    {
        Builder {
            extractor: Chain::new(self.extractor, other),
            allowed_methods: self.allowed_methods,
        }
    }

    /// Creates a `Dispatcher` with the specified function that returns its result immediately.
    pub fn reply<T, F>(self, f: F) -> impl Endpoint<T, Output = F::Out>
    where
        E::Output: Send + 'static,
        E::Error: Send + 'static,
        T: Tuple + Combine<E::Output> + Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out> + Clone + Send + 'static,
        F::Out: Send + 'static,
    {
        let allowed_methods = self.allowed_methods.clone();

        let apply = {
            let extractor = std::sync::Arc::new(self.extractor);
            let allowed_methods = allowed_methods.clone();
            move |method: &Method| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(method))
                {
                    return None;
                }
                let f = f.clone();
                let extractor = extractor.clone();
                Some(crate::endpoint::action(move |input, args: T| {
                    extractor
                        .extract(input)
                        .then(move |result| result.map(|args2| f.call(args.combine(args2))))
                }))
            }
        };

        crate::endpoint::endpoint(apply, allowed_methods)
    }

    /// Creates a `Dispatcher` with the specified function that returns its result as a `Future`.
    pub fn call<T, F, R>(self, f: F) -> impl Endpoint<T, Output = R::Item>
    where
        E::Output: Send + 'static,
        T: Tuple + Combine<E::Output> + Send + 'static,
        F: Func<<T as Combine<E::Output>>::Out, Out = R> + Clone + Send + 'static,
        R: IntoFuture + 'static,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        let allowed_methods = self.allowed_methods.clone();

        let apply = {
            let allowed_methods = allowed_methods.clone();
            let extractor = std::sync::Arc::new(self.extractor);

            move |method: &Method| {
                if allowed_methods
                    .as_ref()
                    .map_or(false, |methods| !methods.contains(method))
                {
                    return None;
                }

                let f = f.clone();
                let extractor = extractor.clone();

                Some(crate::endpoint::action(move |input, args: T| {
                    extractor
                        .extract(input)
                        .map_err(Into::into)
                        .and_then(move |args2| {
                            f.call(args.combine(args2))
                                .into_future()
                                .map_err(Into::into)
                        })
                }))
            }
        };

        crate::endpoint::endpoint(apply, allowed_methods)
    }
}

impl<E> Builder<E>
where
    E: Extractor<Output = ()> + Send + Sync + 'static,
    E::Error: Send + 'static,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<R>(self, output: R) -> impl Endpoint<(), Output = R>
    where
        R: Clone + Send + 'static,
    {
        self.reply(move || output.clone())
    }
}
