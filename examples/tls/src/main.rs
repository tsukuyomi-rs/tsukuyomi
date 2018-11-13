extern crate rustls;
extern crate tsukuyomi;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

const CERTS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/private/cert.pem");
const PRIV_KEY_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/private/key.pem");

fn main() {
    let app = tsukuyomi::app()
        .route(
            tsukuyomi::app::route!() //
                .reply(|| "Hello, Tsukuyomi.\n"),
        ).finish()
        .unwrap();

    tsukuyomi::server(app)
        .bind(tls_transport("127.0.0.1:4000"))
        .run_forever()
        .unwrap();
}

fn tls_transport(addr: &str) -> tsukuyomi::server::transport::TlsConfig<SocketAddr> {
    let addr: SocketAddr = addr.parse().unwrap();

    let client_auth = rustls::NoClientAuth::new();

    let mut config = rustls::ServerConfig::new(client_auth);
    config.key_log = Arc::new(rustls::KeyLogFile::new());

    let certs = load_certs(CERTS_PATH);
    let priv_key = load_private_key(PRIV_KEY_PATH);
    config.set_single_cert(certs, priv_key).unwrap();

    config.set_protocols(&["h2".into(), "http/1.1".into()]);

    tsukuyomi::server::transport::tls(addr, Arc::new(config))
}

fn load_certs(path: impl AsRef<Path>) -> Vec<rustls::Certificate> {
    let certfile = std::fs::File::open(path).unwrap();
    let mut reader = std::io::BufReader::new(certfile);
    rustls::internal::pemfile::certs(&mut reader).unwrap()
}

fn load_private_key(path: impl AsRef<Path>) -> rustls::PrivateKey {
    let rsa_keys = {
        let keyfile = std::fs::File::open(&path).unwrap();
        let mut reader = std::io::BufReader::new(keyfile);
        rustls::internal::pemfile::rsa_private_keys(&mut reader).unwrap()
    };

    let pkcs8_keys = {
        let keyfile = std::fs::File::open(&path).unwrap();
        let mut reader = std::io::BufReader::new(keyfile);
        rustls::internal::pemfile::pkcs8_private_keys(&mut reader).unwrap()
    };

    (pkcs8_keys.into_iter().next())
        .or_else(|| rsa_keys.into_iter().next())
        .expect("invalid private key")
}
