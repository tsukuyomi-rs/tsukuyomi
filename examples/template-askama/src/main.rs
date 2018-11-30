extern crate askama;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

use {
    askama::Template,
    tsukuyomi::{app::directives::*, output::Responder},
};

#[derive(Template, Responder)]
#[template(path = "index.html")]
#[responder(respond_to = "tsukuyomi_askama::respond_to")]
struct Index {
    name: String,
}

fn main() -> tsukuyomi::server::Result<()> {
    App::builder()
        .with(
            route!("/:name") //
                .reply(|name| Index { name }),
        ) //
        .build_server()?
        .run()
}
