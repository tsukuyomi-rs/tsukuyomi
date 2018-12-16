use {
    native_tls::{Identity, TlsAcceptor as NativeTlsAcceptor},
    tokio_tls::TlsAcceptor,
    tsukuyomi::{
        config::prelude::*, //
        App,
        Server,
    },
};

fn main() -> tsukuyomi::server::Result<()> {
    let der = std::fs::read("./private/identity.p12")?;
    let cert = Identity::from_pkcs12(&der, "mypass")?;
    let acceptor = NativeTlsAcceptor::builder(cert).build()?;
    let acceptor = TlsAcceptor::from(acceptor);

    App::create(
        path!(/) //
            .to(endpoint::any() //
                .reply("Hello, Tsukuyomi.\n")),
    ) //
    .map(Server::new)?
    .acceptor(acceptor)
    .run()
}
