extern crate failure;
extern crate native_tls;
extern crate tokio_tls;
extern crate tsukuyomi;

fn main() -> tsukuyomi::server::Result<()> {
    let der = std::fs::read("./private/identity.p12")?;
    let cert = native_tls::Identity::from_pkcs12(&der, "mypass")?;
    let tls_acceptor =
        tokio_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(cert).build()?);

    tsukuyomi::App::builder()
        .with(
            tsukuyomi::app::scope::route!("/") //
                .say("Hello, Tsukuyomi.\n"),
        ) //
        .build_server()?
        .acceptor(tls_acceptor)
        .run()
}
