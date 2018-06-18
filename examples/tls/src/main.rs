extern crate tsukuyomi;

use tsukuyomi::server::transport::TlsConfig;
use tsukuyomi::server::Server;
use tsukuyomi::App;

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/").handle(|_| "Hello, Tsukuyomi.\n");
        })
        .finish()?;

    let server = Server::builder()
        .transport(|t| {
            t.use_tls(TlsConfig {
                certs_path: "private/cert.pem".into(),
                key_path: "private/key.pem".into(),
                alpn_protocols: ["h2", "http/1.1"].into_iter().map(|&s| s.into()).collect(),
            });
        })
        .finish(app)?;

    server.serve();
    Ok(())
}
