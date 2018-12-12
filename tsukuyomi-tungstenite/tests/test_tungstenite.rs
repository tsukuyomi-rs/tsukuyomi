extern crate http;
extern crate tsukuyomi;
extern crate tsukuyomi_tungstenite;
extern crate version_sync;

use {
    http::{
        header::{
            CONNECTION, //
            HOST,
            SEC_WEBSOCKET_ACCEPT,
            SEC_WEBSOCKET_KEY,
            SEC_WEBSOCKET_VERSION,
            UPGRADE,
        },
        Request,
    },
    tsukuyomi::{
        app::config::prelude::*, //
        test::ResponseExt,
        App,
    },
    tsukuyomi_tungstenite::{ws, Ws},
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn test_handshake() -> tsukuyomi::test::Result<()> {
    let app = App::create(
        route().segment("ws")?.to(endpoint::get()
            .extract(ws())
            .reply(|ws: Ws| ws.finish(|_| Ok(())))),
    )?;
    let mut server = tsukuyomi::test::server(app)?;

    let response = server.perform(
        Request::get("/ws")
            .header(HOST, "localhost:4000")
            .header(CONNECTION, "upgrade")
            .header(UPGRADE, "websocket")
            .header(SEC_WEBSOCKET_VERSION, "13")
            .header(SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ=="),
    )?;
    assert_eq!(response.status(), 101);
    assert_eq!(response.header(CONNECTION)?, "upgrade");
    assert_eq!(response.header(UPGRADE)?, "websocket");
    assert_eq!(
        response.header(SEC_WEBSOCKET_ACCEPT)?,
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    );

    Ok(())
}

// TODO: add check whether the task to handle upgraded connection is spawned
