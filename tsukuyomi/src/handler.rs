//! Definition of `Handler`.

use {
    crate::{
        error::Error,
        future::TryFuture,
        util::{Chain, Never, TryFrom}, //
    },
    either::Either,
    http::{header::HeaderValue, HttpTryFrom, Method},
    indexmap::{indexset, IndexSet},
    std::{iter::FromIterator, sync::Arc},
};

/// A set of request methods that a route accepts.
#[derive(Debug, Clone, Default)]
pub struct AllowedMethods(Option<IndexSet<Method>>);

impl AllowedMethods {
    /// Creates an `AllowedMethods` indicating that the all of HTTP methods are accepted.
    pub fn any() -> Self {
        Self::default()
    }

    /// Returns whether this set accepts the all of HTTP methods or not.
    pub fn is_any(&self) -> bool {
        self.0.is_none()
    }

    pub fn contains(&self, method: &Method) -> bool {
        self.0.as_ref().map_or(true, |m| m.contains(method))
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a Method> + 'a {
        self.into_iter()
    }

    pub fn to_header_value(&self) -> HeaderValue {
        let mut bytes = bytes::BytesMut::new();
        let iter = self.iter();
        if let (0, Some(0)) = iter.size_hint() {
            bytes.extend_from_slice(b"*");
        } else {
            for (i, method) in iter.enumerate() {
                if i > 0 {
                    bytes.extend_from_slice(b", ");
                }
                bytes.extend_from_slice(method.as_str().as_bytes());
            }
        }
        unsafe { HeaderValue::from_shared_unchecked(bytes.freeze()) }
    }

    pub fn merge(self, right: Self) -> Self {
        match (self.0, right.0) {
            (Some(mut left), Some(right)) => {
                left.extend(right);
                AllowedMethods(Some(left))
            }
            _ => Self::any(),
        }
    }
}

impl From<Method> for AllowedMethods {
    fn from(method: Method) -> Self {
        AllowedMethods(Some(indexset! { method }))
    }
}

impl FromIterator<Method> for AllowedMethods {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Method>,
    {
        AllowedMethods(Some(FromIterator::from_iter(iter)))
    }
}

impl Extend<Method> for AllowedMethods {
    fn extend<I: IntoIterator<Item = Method>>(&mut self, iterable: I) {
        if let Some(m) = &mut self.0 {
            m.extend(iterable)
        }
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

impl IntoIterator for AllowedMethods {
    type Item = Method;
    type IntoIter = Either<indexmap::set::IntoIter<Method>, std::option::IntoIter<Method>>;

    fn into_iter(self) -> Self::IntoIter {
        match self.0 {
            Some(m) => Either::Left(m.into_iter()),
            None => Either::Right(None.into_iter()),
        }
    }
}

impl<'a> IntoIterator for &'a AllowedMethods {
    type Item = &'a Method;
    type IntoIter = Either<indexmap::set::Iter<'a, Method>, std::option::IntoIter<&'a Method>>;

    fn into_iter(self) -> Self::IntoIter {
        match &self.0 {
            Some(m) => Either::Left(m.iter()),
            None => Either::Right(None.into_iter()),
        }
    }
}

/// A set of metadata associated with the certain `Handler`.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    allowed_methods: AllowedMethods,
}

impl Metadata {
    /// Creates a new `Metadata` with the specified parameters.
    pub fn new(allowed_methods: AllowedMethods) -> Self {
        Self { allowed_methods }
    }

    /// Returns a reference to the inner value of `AllowedMethods`.
    pub fn allowed_methods(&self) -> &AllowedMethods {
        &self.allowed_methods
    }

    /// Returns a mutable reference to the inner value of `AllowedMethods`.
    pub fn allowed_methods_mut(&mut self) -> &mut AllowedMethods {
        &mut self.allowed_methods
    }
}

/// A trait representing the handler associated with the specified endpoint.
pub trait Handler {
    type Output;
    type Error: Into<Error>;
    type Handle: TryFuture<Ok = Self::Output, Error = Self::Error>;

    /// Creates an instance of `Handle` to handle an incoming request.
    fn handle(&self) -> Self::Handle;

    /// Returns the value of `Metadata` associated with this handler.
    fn metadata(&self) -> Metadata;
}

impl<H> Handler for std::rc::Rc<H>
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

    #[inline]
    fn metadata(&self) -> Metadata {
        (**self).metadata()
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

    #[inline]
    fn metadata(&self) -> Metadata {
        (**self).metadata()
    }
}

pub fn handler<T>(
    handle_fn: impl Fn() -> T,
    metadata: Metadata,
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
        metadata: Metadata,
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

        #[inline]
        fn metadata(&self) -> Metadata {
            self.metadata.clone()
        }
    }

    HandlerFn {
        handle_fn,
        metadata,
    }
}

/// A trait representing a type for modifying the instance of `Handler`.
pub trait ModifyHandler<H: Handler> {
    type Output;
    type Handler: Handler<Output = Self::Output>;

    fn modify(&self, input: H) -> Self::Handler;
}

impl<'a, M, H> ModifyHandler<H> for &'a M
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
