//! Components for supporting WebSocket feature.

use futures::IntoFuture;
use http::header::HeaderMap;
use http::{header, Response, StatusCode};
use sha1::{Digest, Sha1};
use tungstenite::protocol::Role;

#[doc(no_inline)]
pub use tokio_tungstenite::WebSocketStream;
#[doc(no_inline)]
pub use tungstenite::protocol::{Message, WebSocketConfig};

use tsukuyomi::error::HttpError;
use tsukuyomi::extractor::{Extract, Extractor, HasExtractor};
use tsukuyomi::input::Input;
use tsukuyomi::output::Responder;
use tsukuyomi::server::service::http::UpgradedIo;

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

impl HttpError for HandshakeError {
    fn status(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
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

/// An `Extractor` which validates the handshake request.
#[derive(Debug, Default)]
pub struct WsExtractor(());

impl Extractor for WsExtractor {
    type Output = (Ws,);
    type Error = HandshakeError;
    type Future = tsukuyomi::extractor::Placeholder<Self::Output, Self::Error>;

    #[inline]
    fn extract(&self, input: &mut Input<'_>) -> Result<Extract<Self>, Self::Error> {
        self::handshake2(input).map(|out| Extract::Ready((out,)))
    }
}

/// The builder for constructing WebSocket response.
#[derive(Debug)]
pub struct Ws {
    accept_hash: String,
    config: Option<WebSocketConfig>,
    extra_headers: Option<HeaderMap>,
}

impl HasExtractor for Ws {
    type Extractor = WsExtractor;

    #[inline]
    fn extractor() -> Self::Extractor {
        WsExtractor::default()
    }
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
    type Error = tsukuyomi::error::ErrorMessage;

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
            .body_mut()
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
