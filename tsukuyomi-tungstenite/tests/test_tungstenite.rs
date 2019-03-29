use {
    http::{
        header::{
            CONNECTION, //
            CONTENT_LENGTH,
            HOST,
            SEC_WEBSOCKET_ACCEPT,
            SEC_WEBSOCKET_KEY,
            SEC_WEBSOCKET_VERSION,
            TRANSFER_ENCODING,
            UPGRADE,
        },
        Request, StatusCode,
    },
    tsukuyomi::{
        endpoint,
        test::{self, loc, TestServer},
        App,
    },
    tsukuyomi_tungstenite::Ws,
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn test_handshake() -> test::Result {
    let app = App::build(|mut s| {
        s.at("/ws")?
            .get()
            .extract(tsukuyomi_tungstenite::ws())
            .to(endpoint::call(|ws: Ws| {
                ws.finish(|_| Ok::<(), std::io::Error>(()))
            }))
    })?;
    let mut server = TestServer::new(app)?;
    let mut client = server.connect();

    client
        .request(
            Request::get("/ws")
                .header(HOST, "localhost:4000")
                .header(CONNECTION, "upgrade")
                .header(UPGRADE, "websocket")
                .header(SEC_WEBSOCKET_VERSION, "13")
                .header(SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
                .body("")?,
        )
        .assert(loc!(), StatusCode::SWITCHING_PROTOCOLS)?
        .assert(loc!(), test::header::not_exists(CONTENT_LENGTH))?
        .assert(loc!(), test::header::not_exists(TRANSFER_ENCODING))?
        .assert(loc!(), test::header::eq(CONNECTION, "upgrade"))?
        .assert(loc!(), test::header::eq(UPGRADE, "websocket"))?
        .assert(
            loc!(),
            test::header::eq(SEC_WEBSOCKET_ACCEPT, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="),
        )?;

    Ok(())
}

// TODO: add check whether the task to handle upgraded connection is spawned
