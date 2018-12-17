use {
    crate::{error::Error, future::TryFuture, handler::AllowedMethods, input::Input},
    http::{Method, StatusCode},
};

/// A trait representing the process to be performed when a route matches.
pub trait Endpoint<T> {
    type Output;
    type Action: EndpointAction<T, Output = Self::Output>;

    /// Determines the `Action` that this endpoint performs based on the request method.
    ///
    /// If the endpoint does not accept the incoming request method, it returns an `Err`.
    fn apply(&self, cx: &mut ApplyContext<'_, '_>) -> Result<Self::Action, ApplyError>;

    /// Returns a list of HTTP methods that this endpoint accepts.
    ///
    /// If it returns a `None`, it means that the endpoint accepts *all* methods.
    ///
    /// This method is called when constructing a `Handler` and used for implementation of
    /// `Handler::allowed_methods`.
    fn allowed_methods(&self) -> Option<AllowedMethods>;
}

#[derive(Debug)]
pub struct ApplyContext<'a, 'task: 'a> {
    input: &'a mut Input<'task>,
}

impl<'a, 'task> ApplyContext<'a, 'task> {
    pub(crate) fn new(input: &'a mut Input<'task>) -> Self {
        Self { input }
    }

    /// Returns HTTP method of the request.
    #[inline]
    pub fn method(&self) -> &Method {
        self.input.request.method()
    }
}

#[derive(Debug)]
pub struct ApplyError(());

impl ApplyError {
    #[inline]
    pub fn method_not_allowed() -> ApplyError {
        ApplyError(())
    }
}

impl From<ApplyError> for Error {
    fn from(_err: ApplyError) -> Self {
        StatusCode::METHOD_NOT_ALLOWED.into()
    }
}

/// A function to create an `Endpoint` from the specified components.
pub fn endpoint<T, A>(
    apply: impl Fn(&mut ApplyContext<'_, '_>) -> Result<A, ApplyError>,
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
        F: Fn(&mut ApplyContext<'_, '_>) -> Result<A, ApplyError>,
        A: EndpointAction<T>,
    {
        type Output = A::Output;
        type Action = A;

        #[inline]
        fn allowed_methods(&self) -> Option<AllowedMethods> {
            self.allowed_methods.clone()
        }

        #[inline]
        fn apply(&self, cx: &mut ApplyContext<'_, '_>) -> Result<Self::Action, ApplyError> {
            (self.apply)(cx)
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
    fn apply(&self, cx: &mut ApplyContext<'_, '_>) -> Result<Self::Action, ApplyError> {
        (**self).apply(cx)
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
    fn apply(&self, cx: &mut ApplyContext<'_, '_>) -> Result<Self::Action, ApplyError> {
        (**self).apply(cx)
    }
}

/// A trait that represents the `Action` created by `Endpoint`.
pub trait EndpointAction<T> {
    type Output;
    type Error: Into<Error>;
    type Future: TryFuture<Ok = Self::Output, Error = Self::Error>;

    /// Promotes this action into a `TryFuture` using the specified value.
    fn invoke(self, args: T) -> Self::Future;
}

/// A function to create an instance of `EndpointAction` from the specified function.
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

        fn invoke(self, args: T) -> Self::Future {
            (self.0)(args)
        }
    }

    EndpointActionFn(f)
}

mod impl_chain {
    use {
        super::{ApplyContext, ApplyError, Endpoint, EndpointAction},
        crate::{
            error::Error,
            future::{Poll, TryFuture},
            handler::AllowedMethods,
            input::Input,
            util::{Chain, Either},
        },
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
        fn apply(&self, cx: &mut ApplyContext<'_, '_>) -> Result<Self::Action, ApplyError> {
            (self.left.apply(cx).map(ChainAction::Left))
                .or_else(|_| self.right.apply(cx).map(ChainAction::Right))
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

        fn invoke(self, args: T) -> Self::Future {
            match self {
                ChainAction::Left(l) => ChainFuture::Left(l.invoke(args)),
                ChainAction::Right(r) => ChainFuture::Right(r.invoke(args)),
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
