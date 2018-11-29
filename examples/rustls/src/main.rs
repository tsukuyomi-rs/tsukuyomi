extern crate failure;
extern crate rustls;
extern crate tokio_rustls;
extern crate tsukuyomi;

use std::{path::Path, sync::Arc};

fn main() -> tsukuyomi::server::Result<()> {
    let tls_acceptor = build_tls_acceptor()?;

    tsukuyomi::App::builder()
        .with(
            tsukuyomi::app::route!() //
                .say("Hello, Tsukuyomi.\n"),
        ) //
        .build_server()?
        .acceptor(tls_acceptor)
        .run()
}

fn build_tls_acceptor() -> failure::Fallible<tokio_rustls::TlsAcceptor> {
    const CERTS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/private/cert.pem");
    const PRIV_KEY_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/private/key.pem");

    let client_auth = rustls::NoClientAuth::new();

    let mut config = rustls::ServerConfig::new(client_auth);
    config.key_log = Arc::new(rustls::KeyLogFile::new());

    let certs = load_certs(CERTS_PATH)?;
    let priv_key = load_private_key(PRIV_KEY_PATH)?;
    config.set_single_cert(certs, priv_key)?;

    config.set_protocols(&["h2".into(), "http/1.1".into()]);

    Ok(Arc::new(config).into())
}

fn load_certs(path: impl AsRef<Path>) -> failure::Fallible<Vec<rustls::Certificate>> {
    let certfile = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(certfile);
    rustls::internal::pemfile::certs(&mut reader)
        .map_err(|_| failure::format_err!("failed to read certificate file"))
}

fn load_private_key(path: impl AsRef<Path>) -> failure::Fallible<rustls::PrivateKey> {
    let rsa_keys = {
        let keyfile = std::fs::File::open(&path)?;
        let mut reader = std::io::BufReader::new(keyfile);
        rustls::internal::pemfile::rsa_private_keys(&mut reader)
            .map_err(|_| failure::format_err!("failed to read private key file as RSA"))?
    };

    let pkcs8_keys = {
        let keyfile = std::fs::File::open(&path)?;
        let mut reader = std::io::BufReader::new(keyfile);
        rustls::internal::pemfile::pkcs8_private_keys(&mut reader)
            .map_err(|_| failure::format_err!("failed to read private key file as PKCS8"))?
    };

    (pkcs8_keys.into_iter().next())
        .or_else(|| rsa_keys.into_iter().next())
        .ok_or_else(|| failure::format_err!("invalid private key"))
}
