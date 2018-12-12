use {
    askama::Template,
    tsukuyomi::{
        app::config::prelude::*, //
        output::Responder,
        App,
    },
};

#[derive(Template, Responder)]
#[template(path = "index.html")]
#[responder(respond_to = "tsukuyomi_askama::respond_to")]
struct Index {
    name: String,
}

fn main() -> tsukuyomi::server::Result<()> {
    App::create({
        path!(/{path::param("name")}) //
            .to({
                endpoint::get() //
                    .reply(|name| Index { name })
            })
    })
    .map(tsukuyomi::server::Server::new)?
    .run()
}
