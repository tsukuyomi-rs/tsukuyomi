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

mod handshake;
mod websocket;
mod ws;

#[doc(no_inline)]
pub use tungstenite::protocol::{Message, WebSocketConfig};

pub use crate::ws::{StreamError, WebSocketStream, Ws};

#[allow(missing_docs)]
pub fn ws() -> impl tsukuyomi::Extractor<
    Output = (Ws,),
    Error = tsukuyomi::Error,
    Extract = impl tsukuyomi::future::TryFuture<Ok = (Ws,), Error = tsukuyomi::Error> + Send + 'static,
> {
    tsukuyomi::extractor::ready(|input| {
        crate::handshake::handshake(input.request) //
            .map(|handshake| (Ws::new(handshake),))
            .map_err(Into::into)
    })
}
