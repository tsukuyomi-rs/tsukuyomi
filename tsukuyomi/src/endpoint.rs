use crate::{
    future::{Future, MaybeFuture},
    handler::AllowedMethods,
    input::Input,
};

pub trait Endpoint<T> {
    type Output;
    type Future: Future<Output = Self::Output> + Send + 'static;

    fn call(self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future>;
}

pub fn endpoint<T, R>(
    f: impl FnOnce(&mut Input<'_>, T) -> MaybeFuture<R>,
) -> impl Endpoint<T, Output = R::Output, Future = R>
where
    R: Future + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    struct EndpointFn<F>(F);

    impl<F, T, R> Endpoint<T> for EndpointFn<F>
    where
        F: FnOnce(&mut Input<'_>, T) -> MaybeFuture<R>,
        R: Future + Send + 'static,
    {
        type Output = R::Output;
        type Future = R;

        fn call(self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future> {
            (self.0)(input, args)
        }
    }

    EndpointFn(f)
}

pub trait Dispatcher<T> {
    type Output;
    type Endpoint: Endpoint<T, Output = Self::Output>;

    /// Returns a list of HTTP methods that the returned endpoint accepts.
    ///
    /// If it returns a `None`, it means that the endpoint accepts *all* methods.
    fn allowed_methods(&self) -> Option<&AllowedMethods>;

    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint>;
}

pub fn dispatcher<T, A>(
    dispatch: impl Fn(&mut Input<'_>) -> Option<A>,
    allowed_methods: Option<AllowedMethods>,
) -> impl Dispatcher<T, Output = A::Output, Endpoint = A>
where
    A: Endpoint<T>,
{
    #[allow(missing_debug_implementations)]
    struct DispatcherFn<F> {
        dispatch: F,
        allowed_methods: Option<AllowedMethods>,
    }

    impl<F, T, A> Dispatcher<T> for DispatcherFn<F>
    where
        F: Fn(&mut Input<'_>) -> Option<A>,
        A: Endpoint<T>,
    {
        type Output = A::Output;
        type Endpoint = A;

        #[inline]
        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.allowed_methods.as_ref()
        }

        #[inline]
        fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
            (self.dispatch)(input)
        }
    }

    DispatcherFn {
        dispatch,
        allowed_methods,
    }
}

impl<E, T> Dispatcher<T> for std::rc::Rc<E>
where
    E: Dispatcher<T>,
{
    type Output = E::Output;
    type Endpoint = E::Endpoint;

    #[inline]
    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
        (**self).dispatch(input)
    }
}

impl<E, T> Dispatcher<T> for std::sync::Arc<E>
where
    E: Dispatcher<T>,
{
    type Output = E::Output;
    type Endpoint = E::Endpoint;

    #[inline]
    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
        (**self).dispatch(input)
    }
}
