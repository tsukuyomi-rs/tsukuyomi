#![allow(missing_docs)]

use {
    crate::{handshake::handshake, websocket::WebSocket},
    futures::{Async, AsyncSink, Future, IntoFuture, Poll, Sink, StartSend, Stream},
    http::Response,
    std::fmt,
    tokio_sync::mpsc,
    tsukuyomi::{
        error::Error,
        future::TryFuture,
        input::Input,
        responder::Responder,
        upgrade::{Upgrade, Upgraded},
    },
    tungstenite::protocol::{Message, Role, WebSocketConfig, WebSocketContext},
};

/// A `Responder` that handles an WebSocket connection.
#[derive(Debug, Clone)]
pub struct Ws<F> {
    on_upgrade: F,
    config: Option<WebSocketConfig>,
}

impl<F, R> Ws<F>
where
    F: FnOnce(WebSocketStream) -> R,
    R: IntoFuture<Item = ()>,
    R::Error: Into<tsukuyomi::upgrade::Error>,
{
    /// Crates a `Ws` with the specified closure.
    pub fn new(on_upgrade: F) -> Self {
        Self {
            on_upgrade,
            config: None,
        }
    }

    /// Sets the configuration of upgraded WebSocket connection.
    pub fn config(self, config: WebSocketConfig) -> Self {
        Self {
            config: Some(config),
            ..self
        }
    }
}

impl<F, R> Responder for Ws<F>
where
    F: FnOnce(WebSocketStream) -> R,
    R: IntoFuture<Item = ()>,
    R::Error: Into<tsukuyomi::upgrade::Error>,
{
    type Response = Response<()>;
    type Upgrade = WsConnection<R::Future>; // private
    type Error = Error;
    type Respond = WsRespond<F>; // private

    fn respond(self) -> Self::Respond {
        WsRespond(Some(self))
    }
}

#[allow(missing_debug_implementations)]
pub struct WsRespond<F>(pub(super) Option<Ws<F>>);

impl<F, R> TryFuture for WsRespond<F>
where
    F: FnOnce(WebSocketStream) -> R,
    R: IntoFuture<Item = ()>,
    R::Error: Into<tsukuyomi::upgrade::Error>,
{
    type Ok = (Response<()>, Option<WsConnection<R::Future>>);
    type Error = tsukuyomi::Error;

    fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        let Ws {
            on_upgrade, config, ..
        } = self.0.take().expect("the future has already been polled");

        let response = handshake(input.request)?.to_response();

        let (tx_recv, rx_recv) = mpsc::unbounded_channel();
        let (tx_send, rx_send) = mpsc::unbounded_channel();
        let foreground = on_upgrade(WebSocketStream { rx_recv, tx_send }).into_future();

        let upgrade = WsConnection {
            foreground: Fuse(Some(foreground)),
            background: Fuse(Some(Background {
                protocol: WebSocketContext::new(Role::Server, config),
                recv: Recv {
                    tx: tx_recv,
                    buf: None,
                },
                send: Send {
                    rx: rx_send,
                    buf: None,
                },
            })),
        };

        Ok((response, Some(upgrade)).into())
    }
}

struct Fuse<Fut>(Option<Fut>);

impl<Fut> Fuse<Fut> {
    fn poll_fuse<F>(&mut self, f: F) -> Poll<(), tsukuyomi::upgrade::Error>
    where
        F: FnOnce(&mut Fut) -> Poll<(), tsukuyomi::upgrade::Error>,
    {
        let res = self.0.as_mut().map(f);
        match res.unwrap_or_else(|| Ok(Async::Ready(()))) {
            res @ Ok(Async::Ready(..)) | res @ Err(..) => {
                self.0 = None;
                res
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
        }
    }
}

#[allow(dead_code)]
#[allow(missing_debug_implementations)]
pub struct WsConnection<Fut> {
    foreground: Fuse<Fut>,
    background: Fuse<Background>,
}

impl<Fut> Upgrade for WsConnection<Fut>
where
    Fut: Future<Item = ()>,
    Fut::Error: Into<tsukuyomi::upgrade::Error>,
{
    fn poll_close(&mut self, stream: &mut dyn Upgraded) -> Poll<(), tsukuyomi::upgrade::Error> {
        let foreground = self
            .foreground
            .poll_fuse(|fut| fut.poll().map_err(Into::into))?;
        let background = self.background.poll_fuse(|fut| fut.poll(stream))?;
        match (foreground, background) {
            (Async::Ready(()), Async::Ready(())) => Ok(Async::Ready(())),
            _ => Ok(Async::NotReady),
        }
    }

    fn shutdown(&mut self) {}
}

#[allow(dead_code)]
#[allow(missing_debug_implementations)]
struct Background {
    protocol: WebSocketContext,
    recv: Recv,
    send: Send,
}

impl Background {
    fn poll(&mut self, stream: &mut dyn Upgraded) -> Poll<(), tsukuyomi::upgrade::Error> {
        let mut socket = WebSocket::new(stream, &mut self.protocol);

        if let Async::Ready(()) = self.recv.poll_close(&mut socket)? {
            self.send.rx.close();
            return Ok(Async::Ready(()));
        }

        self.send
            .poll(&mut socket) //
            .map(|_| Async::NotReady)
    }
}

struct Recv {
    tx: mpsc::UnboundedSender<Message>,
    buf: Option<Message>,
}

impl Recv {
    fn poll_close(&mut self, socket: &mut WebSocket<'_>) -> Poll<(), tsukuyomi::upgrade::Error> {
        if let Some(msg) = self.buf.take() {
            futures::try_ready!(self.try_start_send(msg));
        }

        loop {
            match socket.poll()? {
                Async::Ready(Some(msg)) => futures::try_ready!(self.try_start_send(msg)),
                Async::Ready(None) => return Ok(Async::Ready(())),
                Async::NotReady => {
                    futures::try_ready!(self.tx.poll_complete());
                    return Ok(Async::NotReady);
                }
            }
        }
    }

    fn try_start_send(&mut self, msg: Message) -> Poll<(), tsukuyomi::upgrade::Error> {
        debug_assert!(self.buf.is_none());
        match self.tx.start_send(msg)? {
            AsyncSink::Ready => Ok(Async::Ready(())),
            AsyncSink::NotReady(msg) => {
                self.buf = Some(msg);
                Ok(Async::NotReady)
            }
        }
    }
}

struct Send {
    rx: mpsc::UnboundedReceiver<Message>,
    buf: Option<Message>,
}

impl Send {
    fn poll(&mut self, socket: &mut WebSocket<'_>) -> Poll<(), tsukuyomi::upgrade::Error> {
        if let Some(msg) = self.buf.take() {
            futures::try_ready!(self.try_start_send(socket, msg));
        }

        loop {
            match self.rx.poll()? {
                Async::Ready(Some(msg)) => futures::try_ready!(self.try_start_send(socket, msg)),
                Async::Ready(None) => {
                    futures::try_ready!(socket.close());
                    return Ok(Async::Ready(()));
                }
                Async::NotReady => {
                    futures::try_ready!(socket.poll_complete());
                    return Ok(Async::NotReady);
                }
            }
        }
    }

    fn try_start_send(
        &mut self,
        socket: &mut WebSocket<'_>,
        msg: Message,
    ) -> Poll<(), tsukuyomi::upgrade::Error> {
        debug_assert!(self.buf.is_none());
        match socket.start_send(msg)? {
            AsyncSink::Ready => Ok(Async::Ready(())),
            AsyncSink::NotReady(msg) => {
                self.buf = Some(msg);
                Ok(Async::NotReady)
            }
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct StreamError(StreamErrorKind);

#[derive(Debug)]
enum StreamErrorKind {
    Send(mpsc::error::UnboundedSendError),
    Recv(mpsc::error::UnboundedRecvError),
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            StreamErrorKind::Send(ref e) => e.fmt(f),
            StreamErrorKind::Recv(ref e) => e.fmt(f),
        }
    }
}

impl std::error::Error for StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.0 {
            StreamErrorKind::Send(ref e) => Some(e),
            StreamErrorKind::Recv(ref e) => Some(e),
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug)]
pub struct WebSocketStream {
    tx_send: mpsc::UnboundedSender<Message>,
    rx_recv: mpsc::UnboundedReceiver<Message>,
}

impl Stream for WebSocketStream {
    type Item = Message;
    type Error = StreamError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.rx_recv
            .poll()
            .map_err(|e| StreamError(StreamErrorKind::Recv(e)))
    }
}

impl Sink for WebSocketStream {
    type SinkItem = Message;
    type SinkError = StreamError;

    fn start_send(&mut self, msg: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.tx_send
            .start_send(msg)
            .map_err(|e| StreamError(StreamErrorKind::Send(e)))
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.tx_send
            .poll_complete()
            .map_err(|e| StreamError(StreamErrorKind::Send(e)))
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        self.tx_send
            .close()
            .map_err(|e| StreamError(StreamErrorKind::Send(e)))
    }
}
