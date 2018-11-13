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

fn main() {
    let app = tsukuyomi::app()
        .route(
            tsukuyomi::app::route!("/:name") //
                .reply(|name| Index { name }),
        ) //
        .finish()
        .unwrap();

    tsukuyomi::server(app) //
        .run_forever()
        .unwrap();
}
