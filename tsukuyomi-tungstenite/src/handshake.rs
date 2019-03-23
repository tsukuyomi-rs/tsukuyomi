use {
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
    tsukuyomi::error::HttpError,
};

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

#[derive(Debug)]
pub struct Handshake {
    accept_hash: String,
}

impl Handshake {
    pub fn to_response(&self) -> Response<()> {
        Response::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .header(UPGRADE, "websocket")
            .header(CONNECTION, "upgrade")
            .header(SEC_WEBSOCKET_ACCEPT, &*self.accept_hash)
            .body(())
            .expect("should be a valid response")
    }
}

pub fn handshake<T>(request: &Request<T>) -> Result<Handshake, HandshakeError> {
    match request.headers().get(UPGRADE) {
        Some(h) if h.as_bytes().eq_ignore_ascii_case(b"websocket") => (),
        Some(..) => Err(HandshakeError::InvalidHeader { name: "Upgrade" })?,
        None => Err(HandshakeError::MissingHeader { name: "Upgrade" })?,
    }

    match request.headers().get(CONNECTION) {
        Some(h) if h.as_bytes().eq_ignore_ascii_case(b"upgrade") => (),
        Some(..) => Err(HandshakeError::InvalidHeader { name: "Connection" })?,
        None => Err(HandshakeError::MissingHeader { name: "Connection" })?,
    }

    match request.headers().get(SEC_WEBSOCKET_VERSION) {
        Some(h) if h == "13" => {}
        Some(..) => Err(HandshakeError::InvalidSecWebSocketVersion)?,
        None => Err(HandshakeError::MissingHeader {
            name: "Sec-WebSocket-Version",
        })?,
    }

    let accept_hash = match request.headers().get(SEC_WEBSOCKET_KEY) {
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

    Ok(Handshake { accept_hash })
}
