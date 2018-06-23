#![allow(missing_docs)]

use std::fmt;

use error::Error;
use future::{Future, Poll};
use input::Input;
use output::{Output, Responder};

#[derive(Debug)]
pub struct Handler(HandlerKind);

enum HandlerKind {
    Ready(Box<dyn Fn(&mut Input) -> Result<Output, Error> + Send + Sync>),
    Async(Box<dyn Fn(&mut Input) -> Box<dyn FnMut(&mut Input) -> Poll<Result<Output, Error>> + Send> + Send + Sync>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for HandlerKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HandlerKind::Ready(..) => f.debug_tuple("Ready").finish(),
            HandlerKind::Async(..) => f.debug_tuple("Async").finish(),
        }
    }
}

impl Handler {
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
        Handler(HandlerKind::Ready(Box::new(move |input| {
            handler(input).respond_to(input)
        })))
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
        R::Output: Responder,
    {
        Handler(HandlerKind::Async(Box::new(move |input| {
            let mut future = handler(input);
            Box::new(move |input| input.with_set(|| future.poll()).map(|x| x.respond_to(input)))
        })))
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
    pub fn new_fully_async<R>(handler: impl Fn() -> R + Send + Sync + 'static) -> Handler
    where
        R: Future + Send + 'static,
        R::Output: Responder,
    {
        Handler(HandlerKind::Async(Box::new(move |_| {
            let mut future = handler();
            Box::new(move |input| input.with_set(|| future.poll()).map(|x| x.respond_to(input)))
        })))
    }

    pub fn handle(&self, input: &mut Input) -> Handle {
        match self.0 {
            HandlerKind::Ready(ref f) => Handle(HandleKind::Ready(Some(f(input)))),
            HandlerKind::Async(ref f) => Handle(HandleKind::Async(f(input))),
        }
    }
}

#[derive(Debug)]
pub struct Handle(HandleKind);

enum HandleKind {
    Ready(Option<Result<Output, Error>>),
    Async(Box<dyn FnMut(&mut Input) -> Poll<Result<Output, Error>> + Send>),
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for HandleKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Handle").finish()
    }
}

impl Handle {
    pub fn ok(output: Output) -> Handle {
        Handle(HandleKind::Ready(Some(Ok(output))))
    }

    pub fn err<E>(err: E) -> Handle
    where
        E: Into<Error>,
    {
        Handle(HandleKind::Ready(Some(Err(err.into()))))
    }

    pub(crate) fn poll_ready(&mut self, input: &mut Input) -> Poll<Result<Output, Error>> {
        match self.0 {
            HandleKind::Ready(ref mut res) => Poll::Ready(res.take().expect("this future has already polled")),
            HandleKind::Async(ref mut f) => (f)(input),
        }
    }
}
