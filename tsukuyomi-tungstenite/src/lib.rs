//! The basic WebSocket support for Tsukuyomi, powered by tungstenite.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-tungstenite/0.3.0-dev")]
#![deny(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![doc(test(attr(deny(deprecated, unused,))))]
#![forbid(clippy::unimplemented)]

use {
    futures::IntoFuture,
    http::Response,
    std::marker::PhantomData,
    tokio_io::{AsyncRead, AsyncWrite},
    tsukuyomi::{error::Error, responder::Responder},
};

#[doc(no_inline)]
pub use {
    tokio_tungstenite::WebSocketStream,
    tungstenite::protocol::{Message, WebSocketConfig}, //
};

/// A `Responder` that handles an WebSocket connection.
#[derive(Debug, Clone)]
pub struct Ws<F, I> {
    on_upgrade: F,
    config: Option<WebSocketConfig>,
    _marker: PhantomData<fn(I)>,
}

impl<F, I, R> Ws<F, I>
where
    F: Fn(WebSocketStream<I>) -> R,
    I: AsyncRead + AsyncWrite,
    R: IntoFuture<Item = ()>,
    R::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    /// Crates a `Ws` with the specified closure.
    pub fn new(on_upgrade: F) -> Self {
        Self {
            on_upgrade,
            config: None,
            _marker: PhantomData,
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

impl<F, I, R> Responder for Ws<F, I>
where
    F: Fn(WebSocketStream<I>) -> R,
    I: AsyncRead + AsyncWrite,
    R: IntoFuture<Item = ()>,
    R::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = Response<()>;
    type Upgrade = self::imp::WsUpgrade<F, I>; // private
    type Error = Error;
    type Respond = self::imp::WsRespond<F, I>; // private

    fn respond(self) -> Self::Respond {
        self::imp::WsRespond(Some(self))
    }
}

mod imp {
    use {
        super::{WebSocketConfig, WebSocketStream, Ws},
        futures::IntoFuture,
        http::{
            header::{
                CONNECTION, //
                SEC_WEBSOCKET_ACCEPT,
                SEC_WEBSOCKET_KEY,
                SEC_WEBSOCKET_VERSION,
                UPGRADE,
            },
            Response, StatusCode,
        },
        sha1::{Digest, Sha1},
        std::marker::PhantomData,
        tokio_io::{AsyncRead, AsyncWrite},
        tsukuyomi::{
            error::HttpError,
            future::{Poll, TryFuture},
            input::Input,
            upgrade::Upgrade,
        },
        tungstenite::protocol::Role,
    };

    #[allow(missing_debug_implementations)]
    pub struct WsRespond<F, I>(pub(super) Option<Ws<F, I>>);

    impl<F, I, R> TryFuture for WsRespond<F, I>
    where
        F: FnOnce(WebSocketStream<I>) -> R,
        I: AsyncRead + AsyncWrite,
        R: IntoFuture<Item = ()>,
        R::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        type Ok = (Response<()>, Option<WsUpgrade<F, I>>);
        type Error = tsukuyomi::Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let Ws {
                on_upgrade, config, ..
            } = self.0.take().expect("the future has already been polled");

            let accept_hash = handshake(input)?;

            let response = Response::builder()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .header(UPGRADE, "websocket")
                .header(CONNECTION, "upgrade")
                .header(SEC_WEBSOCKET_ACCEPT, &*accept_hash)
                .body(())
                .expect("should be a valid response");

            let upgrade = WsUpgrade {
                on_upgrade,
                config,
                _marker: PhantomData,
            };

            Ok((response, Some(upgrade)).into())
        }
    }

    #[allow(missing_debug_implementations)]
    pub struct WsUpgrade<F, I> {
        on_upgrade: F,
        config: Option<WebSocketConfig>,
        _marker: PhantomData<fn(I)>,
    }

    impl<F, I, R> Upgrade<I> for WsUpgrade<F, I>
    where
        F: FnOnce(WebSocketStream<I>) -> R,
        I: AsyncRead + AsyncWrite,
        R: IntoFuture<Item = ()>,
        R::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        type Connection = R::Future;

        fn upgrade(self, stream: I) -> Self::Connection {
            let ws_stream = WebSocketStream::from_raw_socket(stream, Role::Server, self.config);
            (self.on_upgrade)(ws_stream).into_future()
        }
    }

    #[derive(Debug, failure::Fail)]
    enum HandshakeError {
        #[fail(display = "The header is missing: `{}'", name)]
        MissingHeader { name: &'static str },

        #[fail(display = "The header value is invalid: `{}'", name)]
        InvalidHeader { name: &'static str },

        #[fail(display = "The value of `Sec-WebSocket-Key` is invalid")]
        InvalidSecWebSocketKey,

        #[fail(display = "The value of `Sec-WebSocket-Version` must be equal to '13'")]
        InvalidSecWebSocketVersion,
    }

    impl HttpError for HandshakeError {
        fn status_code(&self) -> StatusCode {
            StatusCode::BAD_REQUEST
        }
    }

    fn handshake(input: &mut Input<'_>) -> Result<String, HandshakeError> {
        match input.request.headers().get(UPGRADE) {
            Some(h) if h.as_bytes().eq_ignore_ascii_case(b"websocket") => (),
            Some(..) => Err(HandshakeError::InvalidHeader { name: "Upgrade" })?,
            None => Err(HandshakeError::MissingHeader { name: "Upgrade" })?,
        }

        match input.request.headers().get(CONNECTION) {
            Some(h) if h.as_bytes().eq_ignore_ascii_case(b"upgrade") => (),
            Some(..) => Err(HandshakeError::InvalidHeader { name: "Connection" })?,
            None => Err(HandshakeError::MissingHeader { name: "Connection" })?,
        }

        match input.request.headers().get(SEC_WEBSOCKET_VERSION) {
            Some(h) if h == "13" => {}
            Some(..) => Err(HandshakeError::InvalidSecWebSocketVersion)?,
            None => Err(HandshakeError::MissingHeader {
                name: "Sec-WebSocket-Version",
            })?,
        }

        let accept_hash = match input.request.headers().get(SEC_WEBSOCKET_KEY) {
            Some(h) => {
                if h.len() != 24 || {
                    h.as_bytes()
                        .iter()
                        .any(|&b| !b.is_ascii_alphanumeric() && b != b'+' && b != b'/' && b != b'=')
                } {
                    Err(HandshakeError::InvalidSecWebSocketKey)?;
                }

                let mut m = Sha1::new();
                m.input(h.as_bytes());
                m.input(&b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"[..]);
                base64::encode(&*m.result())
            }
            None => Err(HandshakeError::MissingHeader {
                name: "Sec-WebSocket-Key",
            })?,
        };

        // TODO: Sec-WebSocket-Protocol, Sec-WebSocket-Extension

        Ok(accept_hash)
    }
}
