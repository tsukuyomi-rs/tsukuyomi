//! The basic WebSocket support for Tsukuyomi, powered by tungstenite.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-tungstenite/0.1.0")]
#![warn(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    rust_2018_compatibility,
    unused
)]
#![cfg_attr(tsukuyomi_deny_warnings, deny(warnings))]
#![cfg_attr(tsukuyomi_deny_warnings, doc(test(attr(deny(warnings)))))]
#![cfg_attr(feature = "cargo-clippy", warn(pedantic))]
#![cfg_attr(feature = "cargo-clippy", forbid(unimplemented))]

extern crate base64;
extern crate failure;
extern crate futures;
extern crate http;
extern crate sha1;
extern crate tokio_tungstenite;
extern crate tsukuyomi;
extern crate tungstenite;

use {
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
    tsukuyomi::{
        error::{Error, HttpError},
        extractor::Extractor,
        input::{body::UpgradedIo, Input},
        output::Responder,
    },
    tungstenite::protocol::Role,
};

#[doc(no_inline)]
pub use tungstenite::protocol::{Message, WebSocketConfig};

/// A transport for exchanging data frames with the peer.
pub type WebSocketStream = tokio_tungstenite::WebSocketStream<UpgradedIo>;

#[allow(missing_docs)]
#[derive(Debug, failure::Fail)]
pub enum HandshakeError {
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

fn handshake(input: &mut Input<'_>) -> Result<Ws, HandshakeError> {
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

    Ok(Ws {
        accept_hash,
        config: None,
    })
}

/// Create an `Extractor` that handles the WebSocket handshake process and returns a `Ws`.
pub fn ws() -> impl Extractor<Output = (Ws,), Error = HandshakeError> {
    tsukuyomi::extractor::ready(|input| self::handshake(input))
}

/// The builder for constructing WebSocket response.
#[derive(Debug)]
pub struct Ws {
    accept_hash: String,
    config: Option<WebSocketConfig>,
}

impl Ws {
    /// Sets the value of `WebSocketConfig`.
    pub fn config(self, config: WebSocketConfig) -> Self {
        Self {
            config: Some(config),
            ..self
        }
    }

    /// Creates the instance of `Responder` for creating the handshake response.
    ///
    /// This method takes a function to construct the task used after upgrading the protocol.
    pub fn finish<F, R>(self, on_upgrade: F) -> impl Responder
    where
        F: FnOnce(WebSocketStream) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        WsOutput {
            ws: self,
            on_upgrade,
        }
    }
}

#[allow(missing_debug_implementations)]
struct WsOutput<F> {
    ws: Ws,
    on_upgrade: F,
}

impl<F, R> Responder for WsOutput<F>
where
    F: FnOnce(WebSocketStream) -> R + Send + 'static,
    R: IntoFuture<Item = (), Error = ()>,
    R::Future: Send + 'static,
{
    type Body = ();
    type Error = Error;

    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        let Self {
            ws: Ws {
                accept_hash,
                config,
            },
            on_upgrade,
        } = self;

        input
            .upgrade(move |io: UpgradedIo| {
                let transport = WebSocketStream::from_raw_socket(io, Role::Server, config);
                on_upgrade(transport).into_future()
            }).map_err(|_| {
                tsukuyomi::error::internal_server_error("failed to spawn WebSocket task")
            })?;

        Ok(Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "upgrade")
            .header(SEC_WEBSOCKET_ACCEPT, &*accept_hash)
            .body(())
            .expect("should be a valid response"))
    }
}
