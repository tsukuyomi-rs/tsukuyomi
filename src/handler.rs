//! `Handler` and supplemental components.

use either::Either;
use futures::{Async, Future, IntoFuture, Poll};
use std::fmt;
use std::sync::Arc;

use crate::error::Error;
use crate::extract::FromInput;
use crate::input::Input;
use crate::output::{Output, Responder};

pub use self::func::{Func, Tuple};
pub use crate::codegen::{extract, extract_ready};

/// A trait representing handler functions.
pub trait Handler {
    /// Applies an incoming request to this handler.
    fn handle(&self, input: &mut Input<'_>) -> Handle;
}

impl<F> Handler for F
where
    F: Fn(&mut Input<'_>) -> Handle,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> Handle {
        (*self)(input)
    }
}

impl<H> Handler for Arc<H>
where
    H: Handler,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> Handle {
        (**self).handle(input)
    }
}

impl<L, R> Handler for Either<L, R>
where
    L: Handler,
    R: Handler,
{
    #[inline]
    fn handle(&self, input: &mut Input<'_>) -> Handle {
        match self {
            Either::Left(ref handler) => handler.handle(input),
            Either::Right(ref handler) => handler.handle(input),
        }
    }
}

/// A helper function which creates a `Handler` for use as a placeholder.
pub fn unimplemented() -> impl Handler {
    #[allow(missing_debug_implementations)]
    struct Unimplemented;
    impl Handler for Unimplemented {
        #[inline]
        fn handle(&self, _: &mut Input<'_>) -> Handle {
            Handle::ready(Err(crate::error::Failure::internal_server_error(
                failure::format_err!("not implemented yet"),
            ).into()))
        }
    }
    Unimplemented
}

/// A helper function which creates a `Handler` from the specified closure.
pub fn raw(f: impl Fn(&mut Input<'_>) -> Handle) -> impl Handler {
    #[allow(missing_debug_implementations)]
    struct Raw<F>(F);

    impl<F> Handler for Raw<F>
    where
        F: Fn(&mut Input<'_>) -> Handle,
    {
        #[inline]
        fn handle(&self, input: &mut Input<'_>) -> Handle {
            (self.0)(input)
        }
    }

    Raw(f)
}

/// A function which creates a `Handler` from the specified function.
///
/// # Example
///
/// ```ignore
/// fn update_post(id: i32, post: Post)
///     -> impl Future<Error = SomeError, Item = Post>
/// {
///     ...
/// }
///
/// let app = App::builder()
///     .route((
///         "/posts/:id",
///         "PUT",
///         extract(|id: Param<i32>, post: Json<Post>| update_post(id.0, post.0)),
///     ))
///     .finish()?;
/// ```
pub fn extract<F, T>(f: F) -> impl Handler + Send + Sync + 'static
where
    F: Func<T> + Send + Sync + 'static,
    T: FromInput + Tuple + Send + 'static,
    T::Ctx: Send + 'static,
    F::Out: IntoFuture + 'static,
    <F::Out as IntoFuture>::Future: Send + 'static,
    <F::Out as IntoFuture>::Item: Responder,
    Error: From<<F::Out as IntoFuture>::Error>,
{
    let f = Arc::new(f);
    self::raw(move |input| {
        let f = f.clone();
        self::private::handle_async(input, move |arg| f.call(arg).into_future().from_err())
    })
}

/// A function which creates a `Handler` from the specified function.
///
/// Unlike `extract`, the returned value from `f` will be immediately converted to an
/// HTTP response.
pub fn extract_ready<F, T>(f: F) -> impl Handler + Send + Sync + 'static
where
    F: Func<T> + Send + Sync + 'static,
    T: FromInput + Tuple + Send + 'static,
    T::Ctx: Send + 'static,
    F::Out: Responder,
{
    let f = Arc::new(f);
    self::raw(move |input| {
        let f = f.clone();
        self::private::handle_ready(input, move |arg| f.call(arg))
    })
}

// ----------------------------------------------------------------------------

/// A type representing the return value from `Handler::handle`.
pub struct Handle(HandleKind);

#[cfg_attr(feature = "cargo-clippy", allow(large_enum_variant))]
enum HandleKind {
    Ready(Option<Result<Output, Error>>),
    Polling(Box<dyn FnMut(&mut Input<'_>) -> Poll<Output, Error> + Send + 'static>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            HandleKind::Ready(ref res) => f.debug_tuple("Ready").field(res).finish(),
            HandleKind::Polling(..) => f.debug_tuple("Polling").finish(),
        }
    }
}

impl Handle {
    /// Creates a `Handle` from an immediately value.
    pub fn ready(result: Result<Output, Error>) -> Self {
        Handle(HandleKind::Ready(Some(result)))
    }

    /// Creates a `Handle` from a closure representing an asynchronous computation.
    pub fn polling<F>(f: F) -> Self
    where
        F: FnMut(&mut Input<'_>) -> Poll<Output, Error> + Send + 'static,
    {
        Handle(HandleKind::Polling(Box::new(f)))
    }

    #[doc(hidden)]
    #[deprecated(
        since = "0.3.3",
        note = "This method will remove in the future version"
    )]
    #[inline]
    pub fn wrap_async<F>(mut x: F) -> Self
    where
        F: Future + Send + 'static,
        F::Item: Responder,
        Error: From<F::Error>,
    {
        Self::polling(move |input| {
            futures::try_ready!(crate::input::with_set_current(input, || x.poll()))
                .respond_to(input)
                .map(|response| Async::Ready(response.map(Into::into)))
                .map_err(Into::into)
        })
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Output, Error> {
        match self.0 {
            HandleKind::Ready(ref mut res) => res
                .take()
                .expect("this future has already polled")
                .map(Async::Ready),
            HandleKind::Polling(ref mut f) => (f)(input),
        }
    }
}

#[doc(hidden)]
#[deprecated(since = "0.3.3")]
pub fn wrap_ready<R>(f: impl Fn(&mut Input<'_>) -> R) -> impl Handler
where
    R: Responder,
{
    #[allow(missing_debug_implementations)]
    struct ReadyHandler<T>(T);

    impl<T, R> Handler for ReadyHandler<T>
    where
        T: Fn(&mut Input<'_>) -> R,
        R: Responder,
    {
        fn handle(&self, input: &mut Input<'_>) -> Handle {
            Handle::ready(
                (self.0)(input)
                    .respond_to(input)
                    .map(|res| res.map(Into::into))
                    .map_err(Into::into),
            )
        }
    }

    ReadyHandler(f)
}

#[doc(hidden)]
#[deprecated(since = "0.3.3")]
pub fn wrap_async<R>(f: impl Fn(&mut Input<'_>) -> R) -> impl Handler
where
    R: Future + Send + 'static,
    R::Item: Responder,
    Error: From<R::Error>,
{
    #[allow(missing_debug_implementations)]
    struct AsyncHandler<T>(T);

    impl<T, R> Handler for AsyncHandler<T>
    where
        T: Fn(&mut Input<'_>) -> R,
        R: Future + Send + 'static,
        R::Item: Responder,
        Error: From<R::Error>,
    {
        #[allow(deprecated)]
        fn handle(&self, input: &mut Input<'_>) -> Handle {
            Handle::wrap_async((self.0)(input))
        }
    }

    AsyncHandler(f)
}

// not a public API.
#[doc(hidden)]
pub mod private {
    pub use futures::Future;
    use futures::{Async, IntoFuture};

    use super::Handle;
    use crate::error::Error;
    use crate::extract::FromInput;
    use crate::input::Input;
    use crate::output::Responder;

    pub fn handle_ready<F, T, R>(input: &mut Input<'_>, f: F) -> Handle
    where
        F: FnOnce(T) -> R + Send + 'static,
        T: FromInput,
        T: Send + 'static,
        T::Ctx: Send + 'static,
        R: Responder,
    {
        let mut future = input.extract::<T>().map(f);
        Handle::polling(move |input| {
            futures::try_ready!(crate::input::with_set_current(input, || future.poll()))
                .respond_to(input)
                .map(|response| Async::Ready(response.map(Into::into)))
                .map_err(Into::into)
        })
    }

    pub fn handle_async<F, T, R>(input: &mut Input<'_>, f: F) -> Handle
    where
        F: FnOnce(T) -> R + Send + 'static,
        T: FromInput,
        T: Send + 'static,
        T::Ctx: Send + 'static,
        R: IntoFuture<Error = Error> + 'static,
        R::Future: Send + 'static,
        R::Item: Responder,
    {
        let mut future = input.extract::<T>().and_then(f);
        Handle::polling(move |input| {
            futures::try_ready!(crate::input::with_set_current(input, || future.poll()))
                .respond_to(input)
                .map(|response| Async::Ready(response.map(Into::into)))
                .map_err(Into::into)
        })
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(stutter))]
mod func {
    /// A marker trait for constraining the type to tuples.
    pub trait Tuple: TupleSealed {}
    pub trait TupleSealed {}

    impl Tuple for () {}
    impl TupleSealed for () {}

    #[allow(missing_docs)]
    pub trait Func<Args: Tuple>: FuncSealed<Args> {}
    pub trait FuncSealed<Args: Tuple> {
        type Out;
        fn call(&self, args: Args) -> Self::Out;
    }

    impl<F, R> Func<()> for F where F: Fn() -> R {}
    impl<F, R> FuncSealed<()> for F
    where
        F: Fn() -> R,
    {
        type Out = R;
        #[inline]
        fn call(&self, _: ()) -> Self::Out {
            (*self)()
        }
    }

    macro_rules! impl_func {
        ($H:ident, $($T:ident),+) => {
            impl<$H, $($T),+> Tuple for ($H, $($T),+) {}
            impl<$H, $($T),+> TupleSealed for ($H, $($T),+) {}

            impl<F, $H, $($T),+, R> Func<($H, $($T),+)> for F
            where
                F: Fn($H, $($T),+) -> R,
            {
            }
            impl<F, $H, $($T),+, R> FuncSealed<($H, $($T),+)> for F
            where
                F: Fn($H, $($T),+) -> R,
            {
                type Out = R;

                #[inline]
                #[allow(non_snake_case)]
                fn call(&self, ($H, $($T),+): ($H, $($T),+)) -> Self::Out {
                    (*self)($H, $($T),+)
                }
            }

            impl_func!($($T),+);
        };

        ($T:ident) => {
            impl<$T> Tuple for ($T,) {}
            impl<$T> TupleSealed for ($T,) {}

            impl<F, $T, R> Func<($T,)> for F
            where
                F: Fn($T) -> R,
            {
            }
            impl<F, $T, R> FuncSealed<($T,)> for F
            where
                F: Fn($T) -> R,
            {
                type Out = R;

                #[inline]
                #[allow(non_snake_case)]
                fn call(&self, ($T,): ($T,)) -> Self::Out {
                    (*self)($T)
                }
            }
        };
    }

    impl_func!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
}
