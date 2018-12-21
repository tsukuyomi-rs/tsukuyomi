//! The basic WebSocket support for Tsukuyomi, powered by tungstenite.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-tungstenite/0.2.0")]
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
    tsukuyomi::{error::Error, input::body::UpgradedIo, responder::Responder},
};

#[doc(no_inline)]
pub use tungstenite::protocol::{Message, WebSocketConfig};

/// A transport for exchanging data frames with the peer.
pub type WebSocketStream = tokio_tungstenite::WebSocketStream<UpgradedIo>;

/// A `Responder` that handles an WebSocket connection.
#[derive(Debug, Clone)]
pub struct Ws<F> {
    on_upgrade: F,
    config: Option<WebSocketConfig>,
}

impl<F, R> Ws<F>
where
    F: Fn(WebSocketStream) -> R + Send + 'static,
    R: IntoFuture<Item = (), Error = ()>,
    R::Future: Send + 'static,
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
    F: Fn(WebSocketStream) -> R + Send + 'static,
    R: IntoFuture<Item = (), Error = ()>,
    R::Future: Send + 'static,
{
    type Response = Response<()>;
    type Error = Error;
    type Respond = self::imp::WsRespond<F>; // private

    fn respond(self) -> Self::Respond {
        self::imp::WsRespond(Some(self))
    }
}

mod imp {
    use {
        super::{WebSocketStream, Ws},
        futures::{Future, IntoFuture},
        http::{
            header::{
                CONNECTION, //
                SEC_WEBSOCKET_ACCEPT,
                SEC_WEBSOCKET_KEY,
                SEC_WEBSOCKET_VERSION,
                UPGRADE,
            },
            Request, Response, StatusCode,
        },
        sha1::{Digest, Sha1},
        tsukuyomi::{
            error::HttpError,
            future::{Poll, TryFuture},
            input::{
                body::{RequestBody, UpgradedIo},
                Input,
            },
        },
        tsukuyomi_server::rt::{DefaultExecutor, Executor},
        tungstenite::protocol::Role,
    };

    #[allow(missing_debug_implementations)]
    pub struct WsRespond<F>(pub(super) Option<Ws<F>>);

    impl<F, R> TryFuture for WsRespond<F>
    where
        F: FnOnce(WebSocketStream) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        type Ok = Response<()>;
        type Error = tsukuyomi::Error;

        fn poll_ready(&mut self, input: &mut Input<'_>) -> Poll<Self::Ok, Self::Error> {
            let Ws { on_upgrade, config } =
                self.0.take().expect("the future has already been polled");

            let accept_hash = handshake(input)?;

            let body = input
                .locals
                .remove(&RequestBody::KEY) //
                .ok_or_else(|| {
                    tsukuyomi::error::internal_server_error(
                        "the request body has already been stolen by someone",
                    )
                })?;

            let task = body
                .on_upgrade()
                .map_err(|e| log::error!("failed to upgrade the request: {}", e))
                .and_then(move |io: UpgradedIo| {
                    let transport = WebSocketStream::from_raw_socket(io, Role::Server, config);
                    on_upgrade(transport).into_future()
                });

            DefaultExecutor::current()
                .spawn(Box::new(task))
                .map_err(tsukuyomi::error::internal_server_error)?;

            Ok(Response::builder()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .header(UPGRADE, "websocket")
                .header(CONNECTION, "upgrade")
                .header(SEC_WEBSOCKET_ACCEPT, &*accept_hash)
                .body(())
                .expect("should be a valid response")
                .into())
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
        type Body = String;

        fn into_response(self, _: &Request<()>) -> Response<Self::Body> {
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(self.to_string())
                .expect("should be a valid response")
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
                        .into_iter()
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
