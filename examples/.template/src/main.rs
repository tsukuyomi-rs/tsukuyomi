use {
    tsukuyomi::{
        config::prelude::*,
        App,
    },
    izanami::Server,
};

fn main() -> izanami::Result<()> {
    let app = App::create(())?;

    Server::build().start(app)
}
