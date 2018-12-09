//! Definition of `Handler`.

use {
    crate::{
        core::{Chain, Never}, //
        error::Error,
        future::{Async, Future, MaybeFuture, NeverFuture, Poll},
        input::Input,
        output::{Output, Responder},
    },
    std::{fmt, sync::Arc},
};

/// A trait representing the handler associated with the specified endpoint.
pub trait Handler {
    type Output;
    type Future: Future<Output = Self::Output> + Send + 'static;

    /// Creates an `AsyncResult` which handles the incoming request.
    fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future>;
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    type Output = H::Output;
    type Future = H::Future;

    #[inline]
    fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        (**self).call(input)
    }
}

pub fn raw<R>(f: impl Fn(&mut Input<'_>) -> MaybeFuture<R>) -> impl Handler<Output = R::Output>
where
    R: Future + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F, R> Handler for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> MaybeFuture<R>,
        R: Future + Send + 'static,
    {
        type Output = R::Output;
        type Future = R;

        #[inline]
        fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
            (self.0)(input)
        }
    }

    Raw(f)
}

pub fn ready<T: 'static>(f: impl Fn(&mut Input<'_>) -> T) -> impl Handler<Output = T> {
    self::raw(move |input| MaybeFuture::<NeverFuture<_, Never>>::ok(f(input)))
}

// ==== boxed ====

pub(crate) type HandleFn =
    dyn FnMut(&mut crate::future::Context<'_>) -> Poll<Output, Error> + Send + 'static;

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
        f: impl FnMut(&mut crate::future::Context<'_>) -> Poll<Output, Error> + Send + 'static,
    ) -> Self {
        Self {
            inner: HandleInner::PollFn(Box::new(f)),
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
    H: Handler + Send + Sync + 'static,
    H::Output: Responder,
{
    fn from(handler: H) -> Self {
        Self {
            inner: Box::new(move |input| match handler.call(input) {
                MaybeFuture::Ready(Ok(x)) => {
                    Handle::ready(crate::output::internal::respond_to(x, input))
                }
                MaybeFuture::Ready(Err(err)) => Handle::err(err.into()),
                MaybeFuture::Future(mut future) => Handle::poll_fn(move |cx| {
                    let x = futures01::try_ready!(future.poll_ready(cx).map_err(Into::into));
                    crate::output::internal::respond_to(x, &mut *cx.input).map(Async::Ready)
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

/// A trait representing a creator of `Handler`.
pub trait MakeHandler<T> {
    type Output;
    type Handler: Handler<Output = Self::Output>;

    fn make_handler(self, input: T) -> Self::Handler;
}

impl<F, T, H> MakeHandler<T> for F
where
    F: FnOnce(T) -> H,
    H: Handler,
{
    type Output = H::Output;
    type Handler = H;

    #[inline]
    fn make_handler(self, input: T) -> Self::Handler {
        self(input)
    }
}

/// A trait representing a type for modifying the instance of `Handler`.
pub trait ModifyHandler<H: Handler> {
    type Output;
    type Handler: Handler<Output = Self::Output>;

    fn modify(&self, input: H) -> Self::Handler;
}

impl<F, In, Out> ModifyHandler<In> for F
where
    F: Fn(In) -> Out,
    In: Handler,
    Out: Handler,
{
    type Output = Out::Output;
    type Handler = Out;

    #[inline]
    fn modify(&self, input: In) -> Self::Handler {
        (*self)(input)
    }
}

impl<M, H> ModifyHandler<H> for std::rc::Rc<M>
where
    M: ModifyHandler<H>,
    H: Handler,
{
    type Output = M::Output;
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
    type Handler = O::Handler;

    #[inline]
    fn modify(&self, input: H) -> Self::Handler {
        self.right.modify(self.left.modify(input))
    }
}

pub mod modifiers {
    use {
        super::*,
        either::Either,
        http::{Method, StatusCode},
    };

    #[derive(Debug, Default)]
    pub struct DefaultOptions(());

    impl<H> ModifyHandler<H> for DefaultOptions
    where
        H: Handler,
        H::Output: 'static,
    {
        type Output = Either<Output, H::Output>;
        type Handler = DefaultOptionsHandler<H>;

        fn modify(&self, inner: H) -> Self::Handler {
            DefaultOptionsHandler { inner }
        }
    }

    #[doc(hidden)]
    #[derive(Debug)]
    pub struct DefaultOptionsHandler<H> {
        inner: H,
    }

    impl<H> DefaultOptionsHandler<H> {
        fn handle_default_options(&self, input: &mut Input<'_>) -> Option<Output> {
            let resource = (*input.resource)?;
            let allowed_methods = resource.allowed_methods()?;

            if input.request.method() != Method::OPTIONS
                || allowed_methods.contains(&Method::OPTIONS)
            {
                return None;
            }

            let mut output = Output::default();
            *output.status_mut() = StatusCode::NO_CONTENT;
            output
                .headers_mut()
                .insert(http::header::ALLOW, allowed_methods.render_with_options());

            Some(output)
        }
    }

    impl<H> Handler for DefaultOptionsHandler<H>
    where
        H: Handler,
        H::Output: 'static,
    {
        type Output = Either<Output, H::Output>;
        type Future = crate::future::MapOk<H::Future, fn(H::Output) -> Self::Output>;

        fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
            match self.handle_default_options(input) {
                Some(output) => MaybeFuture::ok(Either::Left(output)),
                None => self.inner.call(input).map_ok(Either::Right),
            }
        }
    }
}
