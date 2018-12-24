use {
    tsukuyomi::{
        config::prelude::*,
        App,
    },
    tsukuyomi_server::Server,
};

fn main() -> tsukuyomi_server::Result<()> {
    let app = App::create(())?;

    Server::new(app).run()
}
