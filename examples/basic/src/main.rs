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

    let addr: SocketAddr = "127.0.0.1:4000".parse()?;
    println!("Listening on http://{}", addr);
    Server::bind(addr) //
        .start(app)
}
