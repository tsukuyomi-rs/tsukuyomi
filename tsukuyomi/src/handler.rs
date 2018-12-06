//! Definition of `Handler`.

use {
    crate::{
        common::{Chain, MaybeFuture, Never, NeverFuture},
        error::Error, //
        extractor::Extractor,
        input::Input,
        output::{Output, Responder},
    },
    futures01::{Async, Future, Poll},
    std::{fmt, sync::Arc},
};

/// A trait representing the handler associated with the specified endpoint.
pub trait Handler: Send + Sync + 'static {
    type Output: Responder;
    type Error: Into<Error>;
    type Future: Future<Item = Self::Output, Error = Self::Error> + Send + 'static;

    /// Creates an `AsyncResult` which handles the incoming request.
    fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future>;
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    type Output = H::Output;
    type Error = H::Error;
    type Future = H::Future;

    #[inline]
    fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        (**self).call(input)
    }
}

mod either {
    // impl<L, R> Handler for Either<L, R>
    // where
    //     L: Handler,
    //     R: Handler,
    // {
    //     type Output = Either<L::Output, R::Output>;
    //     type Error = Error;
    //     type Future = EitherFuture<L::Future, R::Future>;

    //     #[inline]
    //     fn handle(&self, input: &mut Input<'_>) -> Handle<Self> {
    //         match self {
    //             Either::Left(ref handler) => Either::Left(handler.handle(input)),
    //             Either::Right(ref handler) => Either::Right(handler.handle(input)),
    //         }
    //     }
    // }
}

pub fn raw<R>(
    f: impl Fn(&mut Input<'_>) -> MaybeFuture<R> + Send + Sync + 'static,
) -> impl Handler<Output = R::Item, Error = R::Error>
where
    R: Future + Send + 'static,
    R::Item: Responder,
    R::Error: Into<Error>,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, R> Handler for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> MaybeFuture<R> + Send + Sync + 'static,
        R: Future + Send + 'static,
        R::Item: Responder,
        R::Error: Into<Error>,
    {
        type Output = R::Item;
        type Error = R::Error;
        type Future = R;

        #[inline]
        fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
            (self.0)(input)
        }
    }

    Raw(f)
}

pub fn ready<T>(
    f: impl Fn(&mut Input<'_>) -> T + Send + Sync + 'static,
) -> impl Handler<Output = T, Error = Never>
where
    T: Responder + 'static,
{
    self::raw(move |input| MaybeFuture::<NeverFuture<_, _>>::ok(f(input)))
}

// ==== boxed ====

pub(crate) type HandleFn = dyn FnMut(&mut Input<'_>) -> Poll<Output, Error> + Send + 'static;

pub(crate) enum HandleInner {
    Ready(Result<Output, Error>),
    PollFn(Box<HandleFn>),
}

impl fmt::Debug for HandleInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandleInner::Ready(result) => f.debug_tuple("Ready").field(result).finish(),
            HandleInner::PollFn(..) => f.debug_tuple("PollFn").finish(),
        }
    }
}

#[derive(Debug)]
pub struct Handle {
    inner: HandleInner,
}

impl Handle {
    pub fn ready(result: Result<Output, Error>) -> Self {
        Self {
            inner: HandleInner::Ready(result),
        }
    }

    pub fn ok(ok: Output) -> Self {
        Self::ready(Ok(ok))
    }

    pub fn err(err: Error) -> Self {
        Self::ready(Err(err))
    }

    pub fn poll_fn(
        future: impl FnMut(&mut Input<'_>) -> Poll<Output, Error> + Send + 'static,
    ) -> Self {
        Self {
            inner: HandleInner::PollFn(Box::new(future)),
        }
    }

    pub(crate) fn into_inner(self) -> HandleInner {
        self.inner
    }
}

pub(crate) struct BoxedHandler {
    inner: Box<dyn Fn(&mut Input<'_>) -> Handle + Send + Sync + 'static>,
}

impl fmt::Debug for BoxedHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoxedHandler").finish()
    }
}

impl<H> From<H> for BoxedHandler
where
    H: Handler,
{
    fn from(handler: H) -> Self {
        Self {
            inner: Box::new(move |input| match handler.call(input) {
                MaybeFuture::Ready(Ok(x)) => {
                    Handle::ready(crate::output::internal::respond_to(x, input))
                }
                MaybeFuture::Ready(Err(e)) => Handle::err(e.into()),
                MaybeFuture::Future(mut future) => Handle::poll_fn(move |input| {
                    let x = futures01::try_ready!(
                        input.with_set_current(|| future.poll().map_err(Into::into))
                    );
                    crate::output::internal::respond_to(x, input).map(Async::Ready)
                }),
            }),
        }
    }
}

impl BoxedHandler {
    pub(crate) fn call(&self, input: &mut Input<'_>) -> Handle {
        (self.inner)(input)
    }
}

/// A trait representing a factory of `Handler` from an instance of `Extractor`.
pub trait MakeHandler<E: Extractor> {
    type Output: Responder;
    type Error: Into<Error>;
    type Handler: Handler<Output = Self::Output, Error = Self::Error>;

    fn make_handler(self, extractor: E) -> Self::Handler;
}

/// A trait representing a type for modifying the instance of `Handler`.
pub trait ModifyHandler<H: Handler> {
    type Output: Responder;
    type Error: Into<crate::Error>;
    type Handler: Handler<Output = Self::Output, Error = Self::Error>;

    fn modify(&self, input: H) -> Self::Handler;
}

impl<'a, M, H> ModifyHandler<H> for &'a M
where
    M: ModifyHandler<H> + 'a,
    H: Handler,
{
    type Output = M::Output;
    type Error = M::Error;
    type Handler = M::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        (*self).modify(input)
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
