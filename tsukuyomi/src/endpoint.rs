use {
    crate::{error::Error, future::TryFuture, handler::AllowedMethods},
    http::Method,
};

pub trait EndpointAction<T> {
    type Output;
    type Error: Into<Error>;
    type Future: TryFuture<Ok = Self::Output, Error = Self::Error>;

    fn call(self, args: T) -> Self::Future;
}

pub fn action<T, R>(
    f: impl FnOnce(T) -> R,
) -> impl EndpointAction<T, Output = R::Ok, Error = R::Error, Future = R>
where
    R: TryFuture,
{
    #[allow(missing_debug_implementations)]
    struct EndpointActionFn<F>(F);

    impl<F, T, R> EndpointAction<T> for EndpointActionFn<F>
    where
        F: FnOnce(T) -> R,
        R: TryFuture,
    {
        type Output = R::Ok;
        type Error = R::Error;
        type Future = R;

        fn call(self, args: T) -> Self::Future {
            (self.0)(args)
        }
    }

    EndpointActionFn(f)
}

pub trait Endpoint<T> {
    type Output;
    type Action: EndpointAction<T, Output = Self::Output>;

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
    A: EndpointAction<T>,
{
    #[allow(missing_debug_implementations)]
    struct ApplyFn<F> {
        apply: F,
        allowed_methods: Option<AllowedMethods>,
    }

    impl<F, T, A> Endpoint<T> for ApplyFn<F>
    where
        F: Fn(&Method) -> Option<A>,
        A: EndpointAction<T>,
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
        crate::{
            error::Error,
            future::{Poll, TryFuture},
            handler::AllowedMethods,
            input::Input,
            util::{Chain, Either},
        },
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

        fn call(self, args: T) -> Self::Future {
            match self {
                ChainAction::Left(l) => ChainFuture::Left(l.call(args)),
                ChainAction::Right(r) => ChainFuture::Right(r.call(args)),
            }
        }
    }

    #[derive(Debug)]
    pub enum ChainFuture<L, R> {
        Left(L),
        Right(R),
    }

    impl<L, R> TryFuture for ChainFuture<L, R>
    where
        L: TryFuture,
        R: TryFuture,
    {
        type Ok = Either<L::Ok, R::Ok>;
        type Error = Error;

        #[inline]
        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            match self {
                ChainFuture::Left(l) => l
                    .poll_ready(input)
                    .map(|x| x.map(Either::Left))
                    .map_err(Into::into),
                ChainFuture::Right(r) => r
                    .poll_ready(input)
                    .map(|x| x.map(Either::Right))
                    .map_err(Into::into),
            }
        }
    }
}
