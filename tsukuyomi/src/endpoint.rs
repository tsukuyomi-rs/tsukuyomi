use crate::{
    future::{Future, MaybeFuture},
    generic::Tuple,
    input::Input,
};

pub trait Endpoint<T: Tuple> {
    type Output;
    type Future: Future<Output = Self::Output> + Send + 'static;

    fn call(&self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future>;
}

pub fn raw<T, R>(
    f: impl Fn(&mut Input<'_>, T) -> MaybeFuture<R> + Clone,
) -> impl Endpoint<T, Output = R::Output, Future = R> + Clone
where
    T: Tuple,
    R: Future + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    #[derive(Clone)]
    struct Raw<F>(F);

    impl<F, T, R> Endpoint<T> for Raw<F>
    where
        F: Fn(&mut Input<'_>, T) -> MaybeFuture<R>,
        T: Tuple,
        R: Future + Send + 'static,
    {
        type Output = R::Output;
        type Future = R;

        fn call(&self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future> {
            (self.0)(input, args)
        }
    }

    Raw(f)
}

impl<E, T: Tuple> Endpoint<T> for std::rc::Rc<E>
where
    E: Endpoint<T>,
{
    type Output = E::Output;
    type Future = E::Future;

    fn call(&self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future> {
        (**self).call(input, args)
    }
}

impl<E, T: Tuple> Endpoint<T> for std::sync::Arc<E>
where
    E: Endpoint<T>,
{
    type Output = E::Output;
    type Future = E::Future;

    fn call(&self, input: &mut Input<'_>, args: T) -> MaybeFuture<Self::Future> {
        (**self).call(input, args)
    }
}
