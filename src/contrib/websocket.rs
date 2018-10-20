//! Components for supporting WebSocket feature.
//!
//! # Examples
//!
//! ```
//! # extern crate futures;
//! # extern crate tsukuyomi;
//! # use futures::prelude::*;
//! # use tsukuyomi::app::App;
//! # use tsukuyomi::input::Input;
//! # use tsukuyomi::output::Responder;
//! # use tsukuyomi::handler::wrap_ready;
//! use tsukuyomi::contrib::websocket::{start, Message};
//!
//! fn websocket(input: &mut Input) -> impl Responder {
//!     start(input, None, |transport| {
//!         let (sink, stream) = transport.split();
//!         stream
//!             .filter_map(|m| {
//!                 println!("Message from client: {:?}", m);
//!                 match m {
//!                     Message::Ping(p) => Some(Message::Pong(p)),
//!                     Message::Pong(_) => None,
//!                     _ => Some(m),
//!                 }
//!             })
//!             .forward(sink)
//!             .then(|_| Ok(()))
//!     })
//! }
//!
//! # fn main() -> tsukuyomi::app::AppResult<()> {
//! let app = App::builder()
//!     .route(("/ws", wrap_ready(websocket)))
//!     .finish()?;
//! # drop(app);
//! # Ok(())
//! # }
//! ```

use base64;
use futures::prelude::*;
use http::{header, Response, StatusCode};
use sha1;

use tokio_tungstenite::WebSocketStream;
pub use tungstenite::protocol::Message;
use tungstenite::protocol::{Role, WebSocketConfig};

use crate::error::{Error, HttpError};
use crate::input::Input;
use crate::output::Responder;
use crate::server::service::http::{Body, UpgradedIo};

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

/// Creates a handshake response from the specified request.
pub fn handshake(input: &mut Input<'_>) -> Result<Response<()>, HandshakeError> {
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

            let mut m = sha1::Sha1::new();
            m.update(h.as_bytes());
            m.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
            base64::encode(&m.digest().bytes()[..])
        }
        None => Err(HandshakeError::MissingHeader {
            name: "Sec-WebSocket-Key",
        })?,
    };

    // TODO: Sec-WebSocket-Protocol, Sec-WebSocket-Extension

    Ok(Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "upgrade")
        .header(header::SEC_WEBSOCKET_ACCEPT, &*accept_hash)
        .body(())
        .expect("Failed to construct a handshake response (This is a bug)"))
}

/// A transport for exchanging data frames with the peer.
pub type Transport = WebSocketStream<UpgradedIo>;

/// A helper function for creating a WebSocket endpoint.
pub fn start<R>(
    input: &mut Input<'_>,
    config: Option<WebSocketConfig>,
    f: impl FnOnce(Transport) -> R + Send + 'static,
) -> impl Responder
where
    R: IntoFuture<Item = (), Error = ()>,
    R::Future: Send + 'static,
{
    let response = handshake(input)?.map(|_| Body::default());

    input
        .body_mut()
        .upgrade(move |io: UpgradedIo| {
            let transport = WebSocketStream::from_raw_socket(io, Role::Server, config);
            f(transport).into_future()
        }).map_err(|_| crate::error::internal_server_error("failed to spawn WebSocket task"))?;

    Ok::<_, Error>(response)
}
