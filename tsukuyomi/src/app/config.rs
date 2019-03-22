use {
    super::{
        recognizer::Recognizer,
        scope::{ScopeId, Scopes},
        AppBase, AppInner, Endpoint, ScopeData, Uri,
    },
    crate::{
        handler::{Handler, ModifyHandler},
        util::{Chain, Never},
    },
    std::{error, fmt, marker::PhantomData, rc::Rc, sync::Arc},
};

/// A type alias of `Result<T, E>` whose error type is restricted to `AppError`.
pub type Result<T> = std::result::Result<T, Error>;

/// An error type which will be thrown from `AppBuilder`.
#[derive(Debug)]
pub struct Error {
    cause: failure::Compat<failure::Error>,
}

impl From<Never> for Error {
    fn from(never: Never) -> Self {
        match never {}
    }
}

impl Error {
    pub fn custom<E>(cause: E) -> Self
    where
        E: Into<failure::Error>,
    {
        Self {
            cause: cause.into().compat(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(f)
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// A trait to specify the concurrency of trait objects inside of `AppBase`.
pub trait Concurrency: self::imp::ConcurrencyImpl {}

pub trait IntoStream<S> {
    fn into_stream(self) -> S;
}

mod imp {
    use {
        crate::{input::Input, output::ResponseBody},
        futures01::Poll,
        http::Response,
    };

    pub trait ConcurrencyImpl: 'static {
        type Handler;
        type Handle;

        type BiStream;
        type Upgrade;
        type Connection;

        fn handle(handler: &Self::Handler) -> Self::Handle;
        fn poll_ready(
            handle: &mut Self::Handle,
            input: &mut Input<'_>,
        ) -> Poll<(Response<ResponseBody>, Option<Self::Upgrade>), crate::error::Error>;

        fn upgrade(upgrade: Self::Upgrade, stream: Self::BiStream) -> Self::Connection;
        fn connection_poll_close(
            conn: &mut Self::Connection,
        ) -> Poll<(), Box<dyn std::error::Error + Send + Sync>>;
        fn connection_shutdown(conn: &mut Self::Connection);
    }
}

/// The implementor of `Concurrency` which means that `App` is thread safe.
#[allow(missing_debug_implementations)]
pub struct ThreadSafe(());

mod thread_safe {
    use {
        crate::{
            error::Error,
            future::{Async, Poll, TryFuture},
            handler::Handler,
            input::Input,
            output::{IntoResponse, ResponseBody},
            responder::Responder,
            upgrade::{Connection, Upgrade},
        },
        http::Response,
        std::{fmt, io},
        tokio_io::{AsyncRead, AsyncWrite},
    };

    impl super::Concurrency for super::ThreadSafe {}

    impl super::imp::ConcurrencyImpl for super::ThreadSafe {
        type Handler = BoxedHandler;
        type Handle = Box<BoxedHandle>;

        type BiStream = BiStream;
        type Upgrade = BoxedUpgrade;
        type Connection = Box<dyn BoxedConnection>;

        fn handle(handler: &Self::Handler) -> Self::Handle {
            (handler.0)()
        }

        fn poll_ready(
            handle: &mut Self::Handle,
            input: &mut Input<'_>,
        ) -> Poll<(Response<ResponseBody>, Option<Self::Upgrade>), Error> {
            (handle)(input)
        }

        fn upgrade(upgrade: Self::Upgrade, stream: Self::BiStream) -> Self::Connection {
            upgrade.upgrade(stream)
        }

        fn connection_poll_close(
            conn: &mut Self::Connection,
        ) -> Poll<(), Box<dyn std::error::Error + Send + Sync>> {
            conn.poll_close()
        }

        fn connection_shutdown(conn: &mut Self::Connection) {
            conn.shutdown();
        }
    }

    type BoxedHandle = dyn FnMut(
            &mut Input<'_>,
        ) -> Poll<
            (Response<ResponseBody>, Option<BoxedUpgrade>),
            crate::error::Error,
        > + Send
        + 'static;

    pub struct BoxedHandler(Box<dyn Fn() -> Box<BoxedHandle> + Send + Sync + 'static>);

    impl fmt::Debug for BoxedHandler {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("BoxedHandler").finish()
        }
    }

    impl<H, U> From<H> for BoxedHandler
    where
        H: Handler + Send + Sync + 'static,
        H::Output: Responder<Upgrade = U>,
        <H::Output as Responder>::Respond: Send + 'static,
        H::Handle: Send + 'static,
        U: Upgrade<BiStream> + Send + 'static,
        U::Connection: Send + 'static,
    {
        fn from(handler: H) -> Self {
            BoxedHandler(Box::new(move || {
                enum State<A, B> {
                    First(A),
                    Second(B),
                }

                let mut state: State<H::Handle, <H::Output as Responder>::Respond> =
                    State::First(handler.handle());

                Box::new(move |input| loop {
                    state = match state {
                        State::First(ref mut handle) => {
                            let x =
                                futures01::try_ready!(handle.poll_ready(input).map_err(Into::into));
                            State::Second(x.respond())
                        }
                        State::Second(ref mut respond) => {
                            let (res, up) = futures01::try_ready!(respond
                                .poll_ready(input)
                                .map_err(Into::into));

                            let res = res
                                .into_response(input.request)
                                .map_err(Into::into)?
                                .map(Into::into);

                            let up = up.map(Into::into);

                            return Ok(Async::Ready((res, up)));
                        }
                    };
                })
            }))
        }
    }

    pub trait Io: AsyncRead + AsyncWrite + Send + 'static {}
    impl<I: AsyncRead + AsyncWrite + Send + 'static> Io for I {}

    #[allow(missing_debug_implementations)]
    pub struct BiStream(Box<dyn Io>);

    impl<I> super::IntoStream<BiStream> for I
    where
        I: AsyncRead + AsyncWrite + Send + 'static,
    {
        fn into_stream(self) -> BiStream {
            BiStream(Box::new(self))
        }
    }

    impl io::Read for BiStream {
        fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
            self.0.read(dst)
        }
    }

    impl io::Write for BiStream {
        fn write(&mut self, src: &[u8]) -> io::Result<usize> {
            self.0.write(src)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    impl AsyncRead for BiStream {}

    impl AsyncWrite for BiStream {
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            self.0.shutdown()
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct BoxedUpgrade(Box<dyn FnMut(BiStream) -> Box<dyn BoxedConnection> + Send + 'static>);

    impl<T> From<T> for BoxedUpgrade
    where
        T: Upgrade<BiStream> + Send + 'static,
        T::Connection: Send + 'static,
    {
        fn from(upgrade: T) -> Self {
            let mut upgrade = Some(upgrade);
            BoxedUpgrade(Box::new(move |stream| {
                let upgrade = upgrade.take().unwrap();
                Box::new(upgrade.upgrade(stream))
            }))
        }
    }

    impl BoxedUpgrade {
        fn upgrade(mut self, stream: BiStream) -> Box<dyn BoxedConnection> {
            (self.0)(stream)
        }
    }

    pub trait BoxedConnection: Send + 'static {
        fn poll_close(&mut self) -> Poll<(), Box<dyn std::error::Error + Send + Sync>>;
        fn shutdown(&mut self);
    }

    impl<C> BoxedConnection for C
    where
        C: Connection + Send + 'static,
    {
        fn poll_close(&mut self) -> Poll<(), Box<dyn std::error::Error + Send + Sync>> {
            Connection::poll_close(self).map_err(Into::into)
        }

        fn shutdown(&mut self) {
            Connection::shutdown(self)
        }
    }
}

/// The implementor of `Concurrency` which means that `App` is *not* thread safe.
#[allow(missing_debug_implementations)]
pub struct CurrentThread(());

mod current_thread {
    use {
        crate::{
            error::Error,
            future::{Async, Poll, TryFuture},
            handler::Handler,
            input::Input,
            output::{IntoResponse, ResponseBody},
            responder::Responder,
            upgrade::{Connection, Upgrade},
        },
        http::Response,
        std::{fmt, io},
        tokio_io::{AsyncRead, AsyncWrite},
    };

    impl super::Concurrency for super::CurrentThread {}

    impl super::imp::ConcurrencyImpl for super::CurrentThread {
        type Handler = BoxedHandler;
        type Handle = Box<BoxedHandle>;
        type BiStream = BiStream;
        type Upgrade = BoxedUpgrade;
        type Connection = Box<dyn BoxedConnection>;

        fn handle(handler: &Self::Handler) -> Self::Handle {
            (handler.0)()
        }

        fn poll_ready(
            handle: &mut Self::Handle,
            input: &mut Input<'_>,
        ) -> Poll<(Response<ResponseBody>, Option<Self::Upgrade>), Error> {
            (handle)(input)
        }

        fn upgrade(upgrade: Self::Upgrade, stream: Self::BiStream) -> Self::Connection {
            upgrade.upgrade(stream)
        }

        fn connection_poll_close(
            conn: &mut Self::Connection,
        ) -> Poll<(), Box<dyn std::error::Error + Send + Sync>> {
            conn.poll_close()
        }

        fn connection_shutdown(conn: &mut Self::Connection) {
            conn.shutdown();
        }
    }

    type BoxedHandle = dyn FnMut(
            &mut Input<'_>,
        ) -> Poll<
            (Response<ResponseBody>, Option<BoxedUpgrade>),
            crate::error::Error,
        > + 'static;

    pub struct BoxedHandler(Box<dyn Fn() -> Box<BoxedHandle> + 'static>);

    impl fmt::Debug for BoxedHandler {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("BoxedHandler").finish()
        }
    }

    impl<H, U> From<H> for BoxedHandler
    where
        H: Handler + 'static,
        H::Output: Responder<Upgrade = U>,
        <H::Output as Responder>::Respond: 'static,
        H::Handle: 'static,
        U: Upgrade<BiStream> + 'static,
        U::Connection: 'static,
    {
        fn from(handler: H) -> Self {
            BoxedHandler(Box::new(move || {
                enum State<A, B> {
                    First(A),
                    Second(B),
                }

                let mut state: State<H::Handle, <H::Output as Responder>::Respond> =
                    State::First(handler.handle());

                Box::new(move |input| loop {
                    state = match state {
                        State::First(ref mut handle) => {
                            let x =
                                futures01::try_ready!(handle.poll_ready(input).map_err(Into::into));
                            State::Second(x.respond())
                        }
                        State::Second(ref mut respond) => {
                            let (res, up) = futures01::try_ready!(respond
                                .poll_ready(input)
                                .map_err(Into::into));

                            let res = res
                                .into_response(input.request)
                                .map_err(Into::into)?
                                .map(Into::into);
                            let up = up.map(Into::into);

                            return Ok(Async::Ready((res, up)));
                        }
                    };
                })
            }))
        }
    }

    pub trait Io: AsyncRead + AsyncWrite + 'static {}
    impl<I: AsyncRead + AsyncWrite + 'static> Io for I {}

    #[allow(missing_debug_implementations)]
    pub struct BiStream(Box<dyn Io>);

    impl<I> super::IntoStream<BiStream> for I
    where
        I: AsyncRead + AsyncWrite + 'static,
    {
        fn into_stream(self) -> BiStream {
            BiStream(Box::new(self))
        }
    }

    impl io::Read for BiStream {
        fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
            self.0.read(dst)
        }
    }

    impl io::Write for BiStream {
        fn write(&mut self, src: &[u8]) -> io::Result<usize> {
            self.0.write(src)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    impl AsyncRead for BiStream {}

    impl AsyncWrite for BiStream {
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            self.0.shutdown()
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct BoxedUpgrade(Box<dyn FnMut(BiStream) -> Box<dyn BoxedConnection> + 'static>);

    impl<T> From<T> for BoxedUpgrade
    where
        T: Upgrade<BiStream> + 'static,
        T::Connection: 'static,
    {
        fn from(upgrade: T) -> Self {
            let mut upgrade = Some(upgrade);
            BoxedUpgrade(Box::new(move |stream| {
                let upgrade = upgrade.take().unwrap();
                Box::new(upgrade.upgrade(stream))
            }))
        }
    }

    impl BoxedUpgrade {
        fn upgrade(mut self, stream: BiStream) -> Box<dyn BoxedConnection> {
            (self.0)(stream)
        }
    }

    pub trait BoxedConnection: 'static {
        fn poll_close(&mut self) -> Poll<(), Box<dyn std::error::Error + Send + Sync>>;
        fn shutdown(&mut self);
    }

    impl<C> BoxedConnection for C
    where
        C: Connection + 'static,
    {
        fn poll_close(&mut self) -> Poll<(), Box<dyn std::error::Error + Send + Sync>> {
            Connection::poll_close(self).map_err(Into::into)
        }

        fn shutdown(&mut self) {
            Connection::shutdown(self)
        }
    }
}

impl<T> AppBase<T>
where
    T: Concurrency,
{
    /// Creates a new `App` from the provided configuration.
    pub fn create(config: impl Config<(), T>) -> Result<Self> {
        let mut recognizer = Recognizer::default();
        let mut scopes = Scopes::new(ScopeData {
            prefix: Uri::root(),
            default_handler: None,
        });
        config
            .configure(&mut Scope {
                recognizer: &mut recognizer,
                scopes: &mut scopes,
                scope_id: ScopeId::root(),
                modifier: &(),
                _marker: PhantomData,
            })
            .map_err(Into::into)?;

        Ok(Self {
            inner: Arc::new(AppInner { recognizer, scopes }),
        })
    }
}

/// A type representing the contextual information in `Config::configure`.
#[derive(Debug)]
pub struct Scope<'a, M, T: Concurrency> {
    recognizer: &'a mut Recognizer<Arc<Endpoint<T>>>,
    scopes: &'a mut Scopes<ScopeData<T>>,
    modifier: &'a M,
    scope_id: ScopeId,
    _marker: PhantomData<Rc<()>>,
}

impl<'a, M, T> Scope<'a, M, T>
where
    T: Concurrency,
{
    /// Adds a route onto the current scope.
    pub fn route<H>(&mut self, handler: H) -> Result<()>
    where
        H: Handler,
        M: ModifyHandler<H>,
        M::Handler: Into<T::Handler>,
    {
        let handler = self.modifier.modify(handler);

        if let Some(path) = handler.metadata().path().cloned() {
            let uri = self.scopes[self.scope_id]
                .data
                .prefix
                .join(&path)
                .map_err(Error::custom)?;

            let scope = &self.scopes[self.scope_id];
            self.recognizer
                .insert(
                    uri.as_str(),
                    Arc::new(Endpoint {
                        scope: scope.id(),
                        ancestors: scope
                            .ancestors()
                            .iter()
                            .cloned()
                            .chain(Some(scope.id()))
                            .collect(),
                        uri: uri.clone(),
                        handler: handler.into(),
                    }),
                )
                .map_err(Error::custom)?;
        } else {
            self.scopes[self.scope_id].data.default_handler = Some(handler.into());
        }

        Ok(())
    }

    /// Creates a sub-scope with the provided prefix onto the current scope.
    pub fn mount(&mut self, prefix: impl AsRef<str>, config: impl Config<M, T>) -> Result<()> {
        let prefix: Uri = prefix.as_ref().parse().map_err(Error::custom)?;

        let scope_id = self
            .scopes
            .add_node(self.scope_id, {
                let parent = &self.scopes[self.scope_id].data;
                ScopeData {
                    prefix: parent.prefix.join(&prefix).map_err(Error::custom)?,
                    default_handler: None,
                }
            })
            .map_err(Error::custom)?;

        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id,
                modifier: &*self.modifier,
                _marker: PhantomData,
            })
            .map_err(Into::into)?;

        Ok(())
    }

    /// Applies the specified configuration with a `ModifyHandler` on the current scope.
    pub fn modify<M2>(
        &mut self,
        modifier: M2,
        config: impl Config<Chain<&'a M, M2>, T>,
    ) -> Result<()> {
        config
            .configure(&mut Scope {
                recognizer: &mut *self.recognizer,
                scopes: &mut *self.scopes,
                scope_id: self.scope_id,
                modifier: &Chain::new(self.modifier, modifier),
                _marker: PhantomData,
            })
            .map_err(Into::into)
    }
}

/// A marker trait annotating that the implementator has an implementation of `Config<M, C>`
/// for a certain `M` and `C`.
pub trait IsConfig {}

/// A trait that abstracts the configuring for constructing an instance of `AppBase`.
pub trait Config<M, T: Concurrency>: IsConfig {
    type Error: Into<Error>;

    /// Applies this configuration to the specified context.
    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error>;
}

impl<T1, T2> IsConfig for Chain<T1, T2>
where
    T1: IsConfig,
    T2: IsConfig,
{
}

impl<S1, S2, M, T> Config<M, T> for Chain<S1, S2>
where
    S1: Config<M, T>,
    S2: Config<M, T>,
    T: Concurrency,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        self.left.configure(cx).map_err(Into::into)?;
        self.right.configure(cx).map_err(Into::into)?;
        Ok(())
    }
}

impl<T> IsConfig for Option<T> where T: IsConfig {}

impl<M, S, T> Config<M, T> for Option<S>
where
    S: Config<M, T>,
    T: Concurrency,
{
    type Error = S::Error;

    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        if let Some(scope) = self {
            scope.configure(cx)?;
        }
        Ok(())
    }
}

impl<T, E> IsConfig for std::result::Result<T, E>
where
    T: IsConfig,
    E: Into<Error>,
{
}

impl<M, S, E, T> Config<M, T> for std::result::Result<S, E>
where
    S: Config<M, T>,
    E: Into<Error>,
    T: Concurrency,
{
    type Error = Error;

    fn configure(self, cx: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        self.map_err(Into::into)?.configure(cx).map_err(Into::into)
    }
}

impl IsConfig for () {}

impl<M, T> Config<M, T> for ()
where
    T: Concurrency,
{
    type Error = Never;

    fn configure(self, _: &mut Scope<'_, M, T>) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}
