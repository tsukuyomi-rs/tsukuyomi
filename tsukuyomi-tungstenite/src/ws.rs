#![allow(missing_docs)]

use {
    crate::{handshake::Handshake, websocket::WebSocket},
    futures::{Async, AsyncSink, Future, IntoFuture, Poll, Sink, StartSend, Stream},
    http::Response,
    std::fmt,
    tokio_sync::mpsc,
    tsukuyomi::{
        error::Error,
        future::TryFuture,
        input::Input,
        output::body::ResponseBody,
        responder::Responder,
        upgrade::{Upgrade, Upgraded},
    },
    tungstenite::protocol::{Message, Role, WebSocketConfig, WebSocketContext},
};

#[derive(Debug)]
pub struct Ws {
    handshake: Handshake,
    config: Option<WebSocketConfig>,
}

impl Ws {
    pub(crate) fn new(handshake: Handshake) -> Self {
        Self {
            handshake,
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

    pub fn finish<F, R>(self, on_upgrade: F) -> WsResponder<F>
    where
        F: FnOnce(WebSocketStream) -> R,
        R: IntoFuture<Item = ()>,
        R::Error: Into<tsukuyomi::upgrade::Error>,
    {
        WsResponder {
            on_upgrade,
            handshake: self.handshake,
            config: self.config,
        }
    }
}

/// A `Responder` that handles an WebSocket connection.
#[derive(Debug)]
pub struct WsResponder<F> {
    on_upgrade: F,
    handshake: Handshake,
    config: Option<WebSocketConfig>,
}

impl<F, R> Responder for WsResponder<F>
where
    F: FnOnce(WebSocketStream) -> R,
    R: IntoFuture<Item = ()>,
    R::Error: Into<tsukuyomi::upgrade::Error>,
{
    type Upgrade = WsConnection<R::Future>; // private
    type Error = Error;
    type Respond = WsRespond<F>; // private

    fn respond(self) -> Self::Respond {
        WsRespond { inner: Some(self) }
    }
}

#[allow(missing_debug_implementations)]
pub struct WsRespond<F> {
    inner: Option<WsResponder<F>>,
}

impl<F, R> TryFuture for WsRespond<F>
where
    F: FnOnce(WebSocketStream) -> R,
    R: IntoFuture<Item = ()>,
    R::Error: Into<tsukuyomi::upgrade::Error>,
{
    type Ok = (Response<ResponseBody>, Option<WsConnection<R::Future>>);
    type Error = tsukuyomi::Error;

    fn poll_ready(&mut self, _: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
        let WsResponder {
            on_upgrade,
            config,
            handshake,
            ..
        } = self
            .inner
            .take()
            .expect("the future has already been polled");

        let response = handshake
            .to_response() //
            .map(|_| ResponseBody::empty());

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
    fn poll_upgrade(&mut self, io: &mut Upgraded<'_>) -> Poll<(), tsukuyomi::upgrade::Error> {
        let foreground = self
            .foreground
            .poll_fuse(|fut| fut.poll().map_err(Into::into))?;
        let background = self.background.poll_fuse(|fut| fut.poll(io))?;
        match (foreground, background) {
            (Async::Ready(()), Async::Ready(())) => Ok(Async::Ready(())),
            _ => Ok(Async::NotReady),
        }
    }

    fn close(&mut self) {
        // TODO: shutdown background
    }
}

#[allow(dead_code)]
#[allow(missing_debug_implementations)]
struct Background {
    protocol: WebSocketContext,
    recv: Recv,
    send: Send,
}

impl Background {
    fn poll(&mut self, io: &mut Upgraded<'_>) -> Poll<(), tsukuyomi::upgrade::Error> {
        let mut socket = WebSocket::new(io, &mut self.protocol);

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
    fn poll_close(
        &mut self,
        socket: &mut WebSocket<'_, Upgraded<'_>>,
    ) -> Poll<(), tsukuyomi::upgrade::Error> {
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
    fn poll(
        &mut self,
        socket: &mut WebSocket<'_, Upgraded<'_>>,
    ) -> Poll<(), tsukuyomi::upgrade::Error> {
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
        socket: &mut WebSocket<'_, Upgraded<'_>>,
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

// ==== WebSocketStream ====

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
