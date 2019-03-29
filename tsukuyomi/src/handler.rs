//! Definition of `Handler`.

use {
    crate::{
        error::Error,
        future::TryFuture,
        util::Chain, //
    },
    std::{rc::Rc, sync::Arc},
};

/// A trait representing the handler associated with the specified endpoint.
pub trait Handler {
    type Output;
    type Error: Into<Error>;
    type Handle: TryFuture<Ok = Self::Output, Error = Self::Error>;

    /// Creates an instance of `Handle` to handle an incoming request.
    fn handle(&self) -> Self::Handle;
}

impl<H> Handler for Rc<H>
where
    H: Handler,
{
    type Output = H::Output;
    type Error = H::Error;
    type Handle = H::Handle;

    #[inline]
    fn handle(&self) -> Self::Handle {
        (**self).handle()
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    type Output = H::Output;
    type Error = H::Error;
    type Handle = H::Handle;

    #[inline]
    fn handle(&self) -> Self::Handle {
        (**self).handle()
    }
}

pub fn handler<T>(
    handle_fn: impl Fn() -> T,
) -> impl Handler<
    Output = T::Ok, //
    Error = T::Error,
    Handle = T,
>
where
    T: TryFuture,
{
    #[allow(missing_debug_implementations)]
    struct HandlerFn<F> {
        handle_fn: F,
    }

    impl<F, T> Handler for HandlerFn<F>
    where
        F: Fn() -> T,
        T: TryFuture,
    {
        type Output = T::Ok;
        type Error = T::Error;
        type Handle = T;

        #[inline]
        fn handle(&self) -> Self::Handle {
            (self.handle_fn)()
        }
    }

    HandlerFn { handle_fn }
}

/// A trait representing a type for modifying the instance of `Handler`.
pub trait ModifyHandler<H: Handler> {
    type Output;
    type Error: Into<Error>;
    type Handler: Handler<Output = Self::Output, Error = Self::Error>;

    fn modify(&self, input: H) -> Self::Handler;
}

impl<'a, M, H> ModifyHandler<H> for &'a M
where
    M: ModifyHandler<H>,
    H: Handler,
{
    type Output = M::Output;
    type Error = M::Error;
    type Handler = M::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        (**self).modify(input)
    }
}

impl<M, H> ModifyHandler<H> for std::rc::Rc<M>
where
    M: ModifyHandler<H>,
    H: Handler,
{
    type Output = M::Output;
    type Error = M::Error;
    type Handler = M::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        (**self).modify(input)
    }
}

impl<M, H> ModifyHandler<H> for std::sync::Arc<M>
where
    M: ModifyHandler<H>,
    H: Handler,
{
    type Output = M::Output;
    type Error = M::Error;
    type Handler = M::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        (**self).modify(input)
    }
}

impl<H> ModifyHandler<H> for ()
where
    H: Handler,
{
    type Output = H::Output;
    type Error = H::Error;
    type Handler = H;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        input
    }
}

impl<I, O, H> ModifyHandler<H> for Chain<I, O>
where
    H: Handler,
    I: ModifyHandler<H>,
    O: ModifyHandler<I::Handler>,
{
    type Output = O::Output;
    type Error = O::Error;
    type Handler = O::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        self.right.modify(self.left.modify(input))
    }
}
