extern crate askama;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

use {askama::Template, tsukuyomi::output::Responder};

#[derive(Template, Responder)]
#[template(path = "index.html")]
#[responder(respond_to = "tsukuyomi_askama::respond_to")]
struct Index {
    name: String,
}

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::App::builder()
        .with(
            tsukuyomi::app::scope::route!("/:name") //
                .reply(|name| Index { name }),
        ) //
        .build_server()?
        .run()
}
