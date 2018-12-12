use {
    openssl::ssl::{AlpnError, SslAcceptor, SslFiletype, SslMethod},
    tsukuyomi::{
        app::config::prelude::*, //
        server::Server,
        App,
    },
};

fn main() -> tsukuyomi::server::Result<()> {
    let mut builder = SslAcceptor::mozilla_modern(SslMethod::tls())?;
    builder.set_certificate_file("./private/cert.pem", SslFiletype::PEM)?;
    builder.set_private_key_file("./private/key.pem", SslFiletype::PEM)?;
    builder.set_alpn_protos(b"\x02h2\x08http/1.1")?;
    builder.set_alpn_select_callback(|_, protos| {
        const H2: &[u8] = b"\x02h2";
        if protos.windows(3).any(|window| window == H2) {
            Ok(b"h2")
        } else {
            Err(AlpnError::NOACK)
        }
    });
    let acceptor = builder.build();

    App::configure(
        route().to(endpoint::any() //
            .say("Hello, Tsukuyomi.\n")),
    ) //
    .map(Server::new)?
    .acceptor(acceptor)
    .run()
}
