use {
    izanami::Server,
    std::net::SocketAddr,
    tsukuyomi::{
        config::prelude::*, //
        App,
    },
};

fn main() -> izanami::Result<()> {
    let app = App::create(
        path!("/") //
            .to(endpoint::reply("Hello, world!\n")),
    )?;

    let addr: SocketAddr = ([127, 0, 0, 1], 4000).into();
    let mut server = Server::bind_tcp(&addr)?;

    println!(
        "Listening on http://{}",
        server.transport().get_ref().0.local_addr()?
    );
    server.start(app)
}
