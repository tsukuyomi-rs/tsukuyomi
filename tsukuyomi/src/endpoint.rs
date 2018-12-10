use crate::{
    future::{Future, MaybeFuture},
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

    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint>;
}

pub fn dispatcher<T, A>(
    f: impl Fn(&mut Input<'_>) -> Option<A>,
) -> impl Dispatcher<T, Output = A::Output, Endpoint = A>
where
    A: Endpoint<T>,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, T, A> Dispatcher<T> for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> Option<A>,
        A: Endpoint<T>,
    {
        type Output = A::Output;
        type Endpoint = A;

        #[inline]
        fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
            (self.0)(input)
        }
    }

    Raw(f)
}

impl<E, T> Dispatcher<T> for std::rc::Rc<E>
where
    E: Dispatcher<T>,
{
    type Output = E::Output;
    type Endpoint = E::Endpoint;

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
    fn dispatch(&self, input: &mut Input<'_>) -> Option<Self::Endpoint> {
        (**self).dispatch(input)
    }
}
