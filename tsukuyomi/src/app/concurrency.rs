//! Specification of trait object's concurrency used in `App`.

pub mod current_thread;

use {
    crate::{
        error::Error,
        future::{Async, Poll, TryFuture},
        handler::Handler,
        input::Input,
        output::{IntoResponse, ResponseBody},
        responder::Responder,
        upgrade::{Connection, Upgrade},
        util::{IntoStream, Never},
    },
    http::Response,
    std::{fmt, io},
    tokio_io::{AsyncRead, AsyncWrite},
};

/// A trait to specify the concurrency of trait objects inside of `AppBase`.
pub trait Concurrency: Sized + 'static {
    type Handler;
    type BiStream;

    #[doc(hidden)]
    type Impl: self::imp::ConcurrencyImpl<Concurrency = Self>;
}

pub(super) mod imp {
    use {
        super::Concurrency,
        crate::{input::Input, output::ResponseBody},
        futures01::Poll,
        http::Response,
    };

    pub trait ConcurrencyImpl: Sized + 'static {
        type Concurrency: Concurrency<Impl = Self>;
        type Handle;
        type Upgrade;
        type Connection;

        fn handle(handler: &<Self::Concurrency as Concurrency>::Handler) -> Self::Handle;

        fn poll_ready_handle(
            handle: &mut Self::Handle,
            input: &mut Input<'_>,
        ) -> Poll<(Response<ResponseBody>, Option<Self::Upgrade>), crate::error::Error>;

        fn upgrade(
            upgrade: Self::Upgrade,
            stream: <Self::Concurrency as Concurrency>::BiStream,
        ) -> Self::Connection;

        fn poll_close_connection(
            conn: &mut Self::Connection,
        ) -> Poll<(), Box<dyn std::error::Error + Send + Sync>>;

        fn shutdown_connection(conn: &mut Self::Connection);
    }
}

/// A `Conccurency` used in `App` by default.
#[allow(missing_debug_implementations)]
pub struct DefaultConcurrency(Never);

impl Concurrency for DefaultConcurrency {
    type Impl = Self;
    type Handler = BoxedHandler;
    type BiStream = BiStream;
}

impl self::imp::ConcurrencyImpl for DefaultConcurrency {
    type Concurrency = Self;
    type Handle = Box<BoxedHandle>;
    type Upgrade = BoxedUpgrade;
    type Connection = Box<dyn BoxedConnection>;

    fn handle(handler: &<Self::Concurrency as Concurrency>::Handler) -> Self::Handle {
        (handler.0)()
    }

    fn poll_ready_handle(
        handle: &mut Self::Handle,
        input: &mut Input<'_>,
    ) -> Poll<(Response<ResponseBody>, Option<Self::Upgrade>), Error> {
        (handle)(input)
    }

    fn upgrade(
        upgrade: Self::Upgrade,
        stream: <Self::Concurrency as Concurrency>::BiStream,
    ) -> Self::Connection {
        upgrade.upgrade(stream)
    }

    fn poll_close_connection(
        conn: &mut Self::Connection,
    ) -> Poll<(), Box<dyn std::error::Error + Send + Sync>> {
        conn.poll_close()
    }

    fn shutdown_connection(conn: &mut Self::Connection) {
        conn.shutdown();
    }
}

type BoxedHandle =
    dyn FnMut(
            &mut Input<'_>,
        ) -> Poll<(Response<ResponseBody>, Option<BoxedUpgrade>), crate::error::Error>
        + Send
        + 'static;

pub struct BoxedHandler(Box<dyn Fn() -> Box<BoxedHandle> + Send + Sync + 'static>);

impl fmt::Debug for BoxedHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoxedHandler").finish()
    }
}

impl<H> From<H> for BoxedHandler
where
    H: Handler + Send + Sync + 'static,
    H::Handle: Send + 'static,
    H::Output: Responder,
    <H::Output as Responder>::Respond: Send + 'static,
    <H::Output as Responder>::Upgrade: Upgrade<BiStream> + Send + 'static,
    <<H::Output as Responder>::Upgrade as Upgrade<BiStream>>::Connection: Send + 'static,
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
                        let x = futures01::try_ready!(handle.poll_ready(input).map_err(Into::into));
                        State::Second(x.respond())
                    }
                    State::Second(ref mut respond) => {
                        let (res, up) =
                            futures01::try_ready!(respond.poll_ready(input).map_err(Into::into));

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

trait Io: AsyncRead + AsyncWrite + Send + 'static {}
impl<I: AsyncRead + AsyncWrite + Send + 'static> Io for I {}

#[allow(missing_debug_implementations)]
pub struct BiStream(Box<dyn Io>);

impl<I> IntoStream<BiStream> for I
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

#[doc(hidden)]
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

#[doc(hidden)]
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
