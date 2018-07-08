//! Components for managing the handler functions.

use futures::{Async, Future, Poll};
use std::fmt;

use error::Error;
use input::Input;
use output::{Output, Responder};

/// A type for wrapping the handler function used in the framework.
pub struct Handler(Box<dyn Fn(&mut Input) -> Handle + Send + Sync + 'static>);

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Handler").finish()
    }
}

impl Handler {
    #[doc(hidden)]
    pub fn new(handler: impl Fn(&mut Input) -> Handle + Send + Sync + 'static) -> Handler {
        Handler(Box::new(handler))
    }

    /// Creates a `Handler` from the provided function.
    ///
    /// The provided handler is *fully synchronous*, which means that the provided handler
    /// will return a result and immediately converted into an HTTP response without polling
    /// the asynchronous status.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::handler::Handler;
    /// # use tsukuyomi::router::Router;
    /// fn index(input: &mut Input) -> &'static str {
    ///     "Hello, Tsukuyomi.\n"
    /// }
    ///
    /// let router = Router::builder()
    ///     .mount("/", |m| {
    ///         m.get("/index.html").handle(Handler::new_ready(index));
    ///     })
    ///     .finish();
    /// # assert!(router.is_ok());
    /// ```
    pub fn new_ready<R>(handler: impl Fn(&mut Input) -> R + Send + Sync + 'static) -> Handler
    where
        R: Responder,
    {
        Handler::new(move |input| Handle::ready(handler(input).respond_to(input)))
    }

    /// Creates a `Handler` from the provided function.
    ///
    /// The provided handler is *partially asynchronous*, which means that the handler will
    /// process some tasks by using the provided reference to `Input` and return a future for
    /// processing the remaining task.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate futures;
    /// # extern crate tsukuyomi;
    /// # use tsukuyomi::error::Error;
    /// # use tsukuyomi::input::Input;
    /// # use tsukuyomi::handler::Handler;
    /// # use tsukuyomi::router::Router;
    /// # use futures::Future;
    /// # use futures::future::lazy;
    /// fn handler(input: &mut Input) -> impl Future<Item = String, Error = Error> + Send + 'static {
    ///     let query = input.uri().query().unwrap_or("<empty>").to_owned();
    ///     lazy(move || {
    ///         Ok(format!("query = {}", query))
    ///     })
    /// }
    ///
    /// let router = Router::builder()
    ///     .mount("/", |m| {
    ///         m.get("/posts").handle(Handler::new_async(handler));
    ///     })
    ///     .finish();
    /// # assert!(router.is_ok());
    /// ```
    pub fn new_async<R>(handler: impl Fn(&mut Input) -> R + Send + Sync + 'static) -> Handler
    where
        R: Future + Send + 'static,
        R::Item: Responder,
        Error: From<R::Error>,
    {
        Handler::new(move |input| {
            let mut future = handler(input);
            Handle(HandleKind::Async(Box::new(move |input| {
                let item = try_ready!(input.with_set_current(|| future.poll()));
                item.respond_to(input).map(Async::Ready)
            })))
        })
    }

    /// Creates a `Handler` from the provided async function.
    ///
    /// The provided handler is *fully asynchronous*, which means that the handler will do nothing
    /// and immediately return a **future** which will be resolved as a value to be converted into
    /// an HTTP response.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate futures;
    /// # extern crate tsukuyomi;
    /// # use tsukuyomi::error::Error;
    /// # use tsukuyomi::handler::Handler;
    /// # use tsukuyomi::router::Router;
    /// # use futures::Future;
    /// # use futures::future::lazy;
    /// fn handler() -> impl Future<Item = &'static str, Error = Error> + Send + 'static {
    ///     lazy(|| {
    ///         Ok("Hello, Tsukuyomi.\n")
    ///     })
    /// }
    ///
    /// // uses upcoming async/await syntax
    /// // async fn handler() -> &'static str {
    /// //    "Hello, Tsukuyomi.\n"
    /// // }
    ///
    /// let router = Router::builder()
    ///     .mount("/", |m| {
    ///         m.get("/posts").handle(Handler::new_fully_async(handler));
    ///     })
    ///     .finish();
    /// # assert!(router.is_ok());
    /// ```
    #[inline]
    pub fn new_fully_async<R>(handler: impl Fn() -> R + Send + Sync + 'static) -> Handler
    where
        R: Future + Send + 'static,
        R::Item: Responder,
        Error: From<R::Error>,
    {
        Handler::new_async(move |_| handler())
    }

    /// Calls the underlying handler function with the provided reference to `Input`.
    #[inline]
    pub fn handle(&self, input: &mut Input) -> Handle {
        (self.0)(input)
    }
}

/// A type representing the return value from `Handler::handle`.
#[derive(Debug)]
pub struct Handle(HandleKind);

enum HandleKind {
    Ready(Option<Result<Output, Error>>),
    Async(Box<dyn FnMut(&mut Input) -> Poll<Output, Error> + Send>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for HandleKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Handle").finish()
    }
}

impl Handle {
    /// Creates a `Handle` from an HTTP response.
    pub fn ok(output: Output) -> Handle {
        Handle::ready(Ok(output))
    }

    /// Creates a `Handle` from an error value.
    pub fn err<E>(err: E) -> Handle
    where
        E: Into<Error>,
    {
        Handle::ready(Err(err.into()))
    }

    #[doc(hidden)]
    pub fn ready(result: Result<Output, Error>) -> Handle {
        Handle(HandleKind::Ready(Some(result)))
    }

    /// Creates a `Handle` from a future.
    pub fn async<F>(mut future: F) -> Handle
    where
        F: Future<Item = Output, Error = Error> + Send + 'static,
    {
        Handle(HandleKind::Async(Box::new(move |input| {
            input.with_set_current(|| future.poll())
        })))
    }

    #[doc(hidden)]
    pub fn async_responder<F>(mut future: F) -> Handle
    where
        F: Future + Send + 'static,
        F::Item: Responder,
        Error: From<F::Error>,
    {
        Handle(HandleKind::Async(Box::new(move |input| {
            let x = try_ready!(input.with_set_current(|| future.poll()));
            x.respond_to(input).map(Async::Ready)
        })))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Output, Error> {
        match self.0 {
            HandleKind::Ready(ref mut res) => res.take().expect("this future has already polled").map(Async::Ready),
            HandleKind::Async(ref mut f) => (f)(input),
        }
    }
}
