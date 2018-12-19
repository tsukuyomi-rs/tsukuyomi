use {
    std::net::SocketAddr,
    tsukuyomi::{
        config::prelude::*, //
        App,
    },
    tsukuyomi_server::Server,
};

fn main() -> tsukuyomi_server::Result<()> {
    let app = App::create(
        path!("/") //
            .to(endpoint::any() //
                .reply("Hello, world!\n")),
    )?;

    let addr: SocketAddr = "127.0.0.1:4000".parse()?;
    println!("Listening on http://{}", addr);
    Server::new(app) //
        .bind(addr) //
        .run()
}
