use failure::Error;
use rustls::internal::pemfile;
use rustls::{Certificate, NoClientAuth, PrivateKey, ServerConfig};
use std::path::PathBuf;
use std::{fs, io};

/// A set of values for setting the TLS encryption.
#[derive(Debug)]
pub struct TlsConfig {
    /// The path of certificates.
    pub certs_path: PathBuf,

    /// The path of private key.
    pub key_path: PathBuf,

    /// A list of protocols used by the ALPN negotiation.
    pub alpn_protocols: Vec<String>,
}

pub(super) fn load_config(config: &TlsConfig) -> Result<ServerConfig, Error> {
    let certs = load_certs(&config.certs_path)?;
    let key = load_key(&config.key_path)?;

    let client_verifier = NoClientAuth::new();
    let mut cfg = ServerConfig::new(client_verifier);
    cfg.set_single_cert(certs, key)?;
    cfg.set_protocols(&config.alpn_protocols[..]);

    Ok(cfg)
}

fn load_certs(path: &PathBuf) -> Result<Vec<Certificate>, Error> {
    let certfile = fs::File::open(path)?;
    let mut reader = io::BufReader::new(certfile);
    let certs = pemfile::certs(&mut reader).map_err(|_| format_err!("failed to read certificates"))?;
    Ok(certs)
}

fn load_key(path: &PathBuf) -> Result<PrivateKey, Error> {
    let keyfile = fs::File::open(path)?;
    let mut reader = io::BufReader::new(keyfile);
    let keys = pemfile::pkcs8_private_keys(&mut reader).map_err(|_| format_err!("failed to read private key"))?;
    if keys.is_empty() {
        bail!("empty private key");
    }
    Ok(keys[0].clone())
}
