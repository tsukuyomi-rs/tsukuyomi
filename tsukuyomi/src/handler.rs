//! Definition of `Handler`.

use {
    crate::{
        core::{Chain, Never, TryFrom}, //
        error::Error,
        future::{Async, Future, MaybeFuture, NeverFuture, Poll},
        input::Input,
        output::{Output, Responder},
    },
    http::{header::HeaderValue, HttpTryFrom, Method},
    indexmap::{indexset, IndexSet},
    lazy_static::lazy_static,
    std::{fmt, iter::FromIterator, sync::Arc},
};

/// A set of request methods that a route accepts.
#[derive(Debug, Clone)]
pub struct AllowedMethods(Arc<IndexSet<Method>>);

impl AllowedMethods {
    pub fn get() -> &'static AllowedMethods {
        lazy_static! {
            static ref VALUE: AllowedMethods = AllowedMethods::from(Method::GET);
        }
        &*VALUE
    }
}

impl From<Method> for AllowedMethods {
    fn from(method: Method) -> Self {
        AllowedMethods(Arc::new(indexset! { method }))
    }
}

impl<M> FromIterator<M> for AllowedMethods
where
    M: Into<Method>,
{
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = M>,
    {
        AllowedMethods(Arc::new(iter.into_iter().map(Into::into).collect()))
    }
}

impl AllowedMethods {
    pub fn contains(&self, method: &Method) -> bool {
        self.0.contains(method)
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a Method> + 'a {
        self.0.iter()
    }

    pub fn render_with_options(&self) -> HeaderValue {
        let mut bytes = bytes::BytesMut::new();
        for (i, method) in self.iter().enumerate() {
            if i > 0 {
                bytes.extend_from_slice(b", ");
            }
            bytes.extend_from_slice(method.as_str().as_bytes());
        }
        if !self.0.contains(&Method::OPTIONS) {
            if !self.0.is_empty() {
                bytes.extend_from_slice(b", ");
            }
            bytes.extend_from_slice(b"OPTIONS");
        }

        unsafe { HeaderValue::from_shared_unchecked(bytes.freeze()) }
    }
}

impl TryFrom<Self> for AllowedMethods {
    type Error = Never;

    #[inline]
    fn try_from(methods: Self) -> std::result::Result<Self, Self::Error> {
        Ok(methods)
    }
}

impl TryFrom<Method> for AllowedMethods {
    type Error = Never;

    #[inline]
    fn try_from(method: Method) -> std::result::Result<Self, Self::Error> {
        Ok(AllowedMethods::from(method))
    }
}

impl<M> TryFrom<Vec<M>> for AllowedMethods
where
    Method: HttpTryFrom<M>,
{
    type Error = http::Error;

    #[inline]
    fn try_from(methods: Vec<M>) -> std::result::Result<Self, Self::Error> {
        let methods: Vec<_> = methods
            .into_iter()
            .map(Method::try_from)
            .collect::<std::result::Result<_, _>>()
            .map_err(Into::into)?;
        Ok(AllowedMethods::from_iter(methods))
    }
}

impl<'a> TryFrom<&'a str> for AllowedMethods {
    type Error = failure::Error;

    #[inline]
    fn try_from(methods: &'a str) -> std::result::Result<Self, Self::Error> {
        let methods: Vec<_> = methods
            .split(',')
            .map(|s| Method::try_from(s.trim()).map_err(Into::into))
            .collect::<http::Result<_>>()?;
        Ok(AllowedMethods::from_iter(methods))
    }
}

/// A trait representing the handler associated with the specified endpoint.
pub trait Handler {
    type Output;
    type Future: Future<Output = Self::Output> + Send + 'static;

    /// Returns a list of HTTP methods that this handler accepts.
    ///
    /// If it returns a `None`, it means that the handler accepts *all* methods.
    fn allowed_methods(&self) -> Option<&AllowedMethods>;

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
    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        (**self).allowed_methods()
    }

    #[inline]
    fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
        (**self).call(input)
    }
}

pub fn handler<R>(
    call: impl Fn(&mut Input<'_>) -> MaybeFuture<R>,
    allowed_methods: Option<AllowedMethods>,
) -> impl Handler<Output = R::Output>
where
    R: Future + Send + 'static,
{
    #[allow(missing_debug_implementations)]
    struct HandlerFn<F> {
        call: F,
        allowed_methods: Option<AllowedMethods>,
    }

    impl<F, R> Handler for HandlerFn<F>
    where
        F: Fn(&mut Input<'_>) -> MaybeFuture<R>,
        R: Future + Send + 'static,
    {
        type Output = R::Output;
        type Future = R;

        #[inline]
        fn allowed_methods(&self) -> Option<&AllowedMethods> {
            self.allowed_methods.as_ref()
        }

        #[inline]
        fn call(&self, input: &mut Input<'_>) -> MaybeFuture<Self::Future> {
            (self.call)(input)
        }
    }

    HandlerFn {
        call,
        allowed_methods,
    }
}

pub fn ready<T: 'static>(f: impl Fn(&mut Input<'_>) -> T) -> impl Handler<Output = T> {
    self::handler(
        move |input| MaybeFuture::<NeverFuture<_, Never>>::ok(f(input)),
        None,
    )
}

// ==== boxed ====

pub(crate) type HandleTask =
    dyn FnMut(&mut crate::future::Context<'_>) -> Poll<Output, Error> + Send + 'static;

pub(crate) enum Handle {
    Ready(Result<Output, Error>),
    InFlight(Box<HandleTask>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Handle::Ready(result) => f.debug_tuple("Ready").field(result).finish(),
            Handle::InFlight(..) => f.debug_tuple("InFlight").finish(),
        }
    }
}

pub(crate) trait BoxedHandler {
    fn allowed_methods(&self) -> Option<&AllowedMethods>;
    fn call(&self, input: &mut Input<'_>) -> Handle;
}

impl fmt::Debug for dyn BoxedHandler + Send + Sync + 'static {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoxedHandler").finish()
    }
}

impl<H> BoxedHandler for H
where
    H: Handler + Send + Sync + 'static,
    H::Output: Responder,
{
    fn allowed_methods(&self) -> Option<&AllowedMethods> {
        Handler::allowed_methods(self)
    }

    fn call(&self, input: &mut Input<'_>) -> Handle {
        match Handler::call(self, input) {
            MaybeFuture::Ready(Ok(x)) => {
                Handle::Ready(crate::output::internal::respond_to(x, input))
            }
            MaybeFuture::Ready(Err(err)) => Handle::Ready(Err(err.into())),
            MaybeFuture::Future(mut future) => Handle::InFlight(Box::new(move |cx| {
                let x = futures01::try_ready!(future.poll_ready(cx).map_err(Into::into));
                crate::output::internal::respond_to(x, &mut *cx.input).map(Async::Ready)
            })),
        }
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
