extern crate askama;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

use askama::Template;
use tsukuyomi::output::Responder;

#[derive(Template, Responder)]
#[template(path = "index.html")]
#[responder(respond_to = "tsukuyomi_askama::respond_to")]
struct Index {
    name: String,
}

fn main() -> tsukuyomi::server::Result<()> {
    tsukuyomi::app()
        .route(
            tsukuyomi::app::route!("/:name") //
                .reply(|name| Index { name }),
        ) //
        .build_server()?
        .run_forever()
}
