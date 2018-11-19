//! Components for supporting WebSocket feature.

#![doc(html_root_url = "https://docs.rs/tsukuyomi-websocket/0.1.0")]
#![warn(
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

extern crate tsukuyomi;

extern crate base64;
extern crate failure;
extern crate futures;
extern crate http;
extern crate sha1;
extern crate tokio_tungstenite;
extern crate tungstenite;

use {
    futures::IntoFuture,
    http::{
        header::{self, HeaderMap},
        Response, StatusCode,
    },
    sha1::{Digest, Sha1},
    tsukuyomi::{
        error::Error,
        extractor::Extractor,
        input::{body::UpgradedIo, Input},
        output::Responder,
    },
    tungstenite::protocol::Role,
};

#[doc(no_inline)]
pub use {
    tokio_tungstenite::WebSocketStream,
    tungstenite::protocol::{Message, WebSocketConfig},
};

/// A transport for exchanging data frames with the peer.
pub type Transport = WebSocketStream<UpgradedIo>;

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

fn handshake2(input: &mut Input<'_>) -> Result<Ws, HandshakeError> {
    match input.headers().get(header::UPGRADE) {
        Some(h) if h == "Websocket" || h == "websocket" => (),
        Some(..) => Err(HandshakeError::InvalidHeader { name: "Upgrade" })?,
        None => Err(HandshakeError::MissingHeader { name: "Upgrade" })?,
    }

    match input.headers().get(header::CONNECTION) {
        Some(h) if h == "Upgrade" || h == "upgrade" => (),
        Some(..) => Err(HandshakeError::InvalidHeader { name: "Connection" })?,
        None => Err(HandshakeError::MissingHeader { name: "Connection" })?,
    }

    match input.headers().get(header::SEC_WEBSOCKET_VERSION) {
        Some(h) if h == "13" => {}
        _ => Err(HandshakeError::InvalidSecWebSocketVersion)?,
    }

    let accept_hash = match input.headers().get(header::SEC_WEBSOCKET_KEY) {
        Some(h) => {
            let decoded = base64::decode(h).map_err(|_| HandshakeError::InvalidSecWebSocketKey)?;
            if decoded.len() != 16 {
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
        extra_headers: None,
    })
}

pub fn extractor() -> impl Extractor<Output = (Ws,), Error = Error> {
    tsukuyomi::extractor::ready(|input| {
        self::handshake2(input).map_err(tsukuyomi::error::bad_request)
    })
}

/// The builder for constructing WebSocket response.
#[derive(Debug)]
pub struct Ws {
    accept_hash: String,
    config: Option<WebSocketConfig>,
    extra_headers: Option<HeaderMap>,
}

impl Ws {
    /// Sets the value of `WebSocketConfig`.
    pub fn config(self, config: WebSocketConfig) -> Self {
        Self {
            config: Some(config),
            ..self
        }
    }

    /// Appends a header field to be inserted into the handshake response.
    pub fn with_header(mut self, name: header::HeaderName, value: header::HeaderValue) -> Self {
        self.extra_headers
            .get_or_insert_with(Default::default)
            .append(name, value);
        self
    }

    /// Creates the instance of `Responder` for creating the handshake response.
    ///
    /// This method takes a function to construct the task used after upgrading the protocol.
    pub fn finish<F, R>(self, f: F) -> impl Responder
    where
        F: FnOnce(Transport) -> R + Send + 'static,
        R: IntoFuture<Item = (), Error = ()>,
        R::Future: Send + 'static,
    {
        WsOutput(self, f)
    }
}

#[allow(missing_debug_implementations)]
struct WsOutput<F>(Ws, F);

impl<F, R> Responder for WsOutput<F>
where
    F: FnOnce(Transport) -> R + Send + 'static,
    R: IntoFuture<Item = (), Error = ()>,
    R::Future: Send + 'static,
{
    type Body = ();
    type Error = Error;

    fn respond_to(self, input: &mut Input<'_>) -> Result<Response<Self::Body>, Self::Error> {
        let Self {
            0:
                Ws {
                    accept_hash,
                    config,
                    extra_headers,
                },
            1: on_upgrade,
        } = self;

        input
            .upgrade(move |io: UpgradedIo| {
                let transport = WebSocketStream::from_raw_socket(io, Role::Server, config);
                on_upgrade(transport).into_future()
            }).map_err(|_| {
                tsukuyomi::error::internal_server_error("failed to spawn WebSocket task")
            })?;

        let mut response = Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header(header::UPGRADE, "websocket")
            .header(header::CONNECTION, "upgrade")
            .header(header::SEC_WEBSOCKET_ACCEPT, &*accept_hash)
            .body(())
            .expect("should be a valid response");

        if let Some(hdrs) = extra_headers {
            response.headers_mut().extend(hdrs);
        }

        Ok(response)
    }
}
