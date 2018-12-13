use {
    crate::{error::Error, handler::AllowedMethods, input::Input},
    futures01::{Future, IntoFuture},
    http::Method,
};

pub trait EndpointAction<T> {
    type Output;
    type Error: Into<Error>;
    type Future: Future<Item = Self::Output, Error = Self::Error> + Send + 'static;

    fn call(self, input: &mut Input<'_>, args: T) -> Self::Future;
}

pub fn action<T, R>(
    f: impl FnOnce(&mut Input<'_>, T) -> R,
) -> impl EndpointAction<T, Output = R::Item, Future = R::Future>
where
    R: IntoFuture,
    R::Future: Send + 'static,
    R::Error: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    struct EndpointActionFn<F>(F);

    impl<F, T, R> EndpointAction<T> for EndpointActionFn<F>
    where
        F: FnOnce(&mut Input<'_>, T) -> R,
        R: IntoFuture,
        R::Future: Send + 'static,
        R::Error: Into<Error>,
    {
        type Output = R::Item;
        type Error = R::Error;
        type Future = R::Future;

        fn call(self, input: &mut Input<'_>, args: T) -> Self::Future {
            (self.0)(input, args).into_future()
        }
    }

    EndpointActionFn(f)
}

pub trait Endpoint<T> {
    type Output;
    type Action: EndpointAction<T, Output = Self::Output> + Send + 'static;

    /// Returns a list of HTTP methods that the returned endpoint accepts.
    ///
    /// If it returns a `None`, it means that the endpoint accepts *all* methods.
    fn allowed_methods(&self) -> Option<AllowedMethods>;

    fn apply(&self, method: &Method) -> Option<Self::Action>;
}

pub fn endpoint<T, A>(
    apply: impl Fn(&Method) -> Option<A>,
    allowed_methods: Option<AllowedMethods>,
) -> impl Endpoint<T, Output = A::Output, Action = A>
where
    A: EndpointAction<T> + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    struct ApplyFn<F> {
        apply: F,
        allowed_methods: Option<AllowedMethods>,
    }

    impl<F, T, A> Endpoint<T> for ApplyFn<F>
    where
        F: Fn(&Method) -> Option<A>,
        A: EndpointAction<T> + Send + 'static,
    {
        type Output = A::Output;
        type Action = A;

        #[inline]
        fn allowed_methods(&self) -> Option<AllowedMethods> {
            self.allowed_methods.clone()
        }

        #[inline]
        fn apply(&self, method: &Method) -> Option<Self::Action> {
            (self.apply)(method)
        }
    }

    ApplyFn {
        apply,
        allowed_methods,
    }
}

pub fn allow_any<T, R>(
    f: impl Fn(&mut Input<'_>, T) -> R + Clone + Send + 'static,
) -> impl Endpoint<T, Output = R::Item>
where
    T: 'static,
    R: IntoFuture + 'static,
    R::Future: Send + 'static,
    R::Error: Into<Error>,
{
    endpoint(move |_| Some(action(f.clone())), None)
}

impl<E, T> Endpoint<T> for std::rc::Rc<E>
where
    E: Endpoint<T>,
{
    type Output = E::Output;
    type Action = E::Action;

    #[inline]
    fn allowed_methods(&self) -> Option<AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn apply(&self, method: &Method) -> Option<Self::Action> {
        (**self).apply(method)
    }
}

impl<E, T> Endpoint<T> for std::sync::Arc<E>
where
    E: Endpoint<T>,
{
    type Output = E::Output;
    type Action = E::Action;

    #[inline]
    fn allowed_methods(&self) -> Option<AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn apply(&self, method: &Method) -> Option<Self::Action> {
        (**self).apply(method)
    }
}

mod impl_chain {
    use {
        super::{Endpoint, EndpointAction},
        crate::{core::Chain, error::Error, handler::AllowedMethods, input::Input},
        either::Either,
        futures01::{Future, Poll},
        http::Method,
    };

    impl<L, R, T> Endpoint<T> for Chain<L, R>
    where
        L: Endpoint<T>,
        R: Endpoint<T>,
    {
        type Output = Either<L::Output, R::Output>;
        type Action = ChainAction<L::Action, R::Action>;

        #[inline]
        fn allowed_methods(&self) -> Option<AllowedMethods> {
            let left = self.left.allowed_methods()?;
            let right = self.right.allowed_methods()?;
            Some(left.iter().chain(right.iter()).cloned().collect())
        }

        #[inline]
        fn apply(&self, method: &Method) -> Option<Self::Action> {
            self.left
                .apply(method)
                .map(ChainAction::Left)
                .or_else(|| self.right.apply(method).map(ChainAction::Right))
        }
    }

    #[derive(Debug)]
    pub enum ChainAction<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R, T> EndpointAction<T> for ChainAction<L, R>
    where
        L: EndpointAction<T>,
        R: EndpointAction<T>,
    {
        type Output = Either<L::Output, R::Output>;
        type Error = Error;
        type Future = ChainFuture<L::Future, R::Future>;

        fn call(self, input: &mut Input<'_>, args: T) -> Self::Future {
            match self {
                ChainAction::Left(l) => ChainFuture::Left(l.call(input, args)),
                ChainAction::Right(r) => ChainFuture::Right(r.call(input, args)),
            }
        }
    }

    #[derive(Debug)]
    pub enum ChainFuture<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R> Future for ChainFuture<L, R>
    where
        L: Future,
        R: Future,
        L::Error: Into<Error>,
        R::Error: Into<Error>,
    {
        type Item = Either<L::Item, R::Item>;
        type Error = Error;

        #[inline]
        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            match self {
                ChainFuture::Left(l) => l.poll().map(|x| x.map(Either::Left)).map_err(Into::into),
                ChainFuture::Right(r) => r.poll().map(|x| x.map(Either::Right)).map_err(Into::into),
            }
        }
    }
}

// ==== builder ====
