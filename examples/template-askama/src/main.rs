extern crate askama;
extern crate tsukuyomi;
extern crate tsukuyomi_askama;

use askama::Template;

#[derive(askama::Template, tsukuyomi_askama::TemplateResponder)]
#[template(path = "index.html")]
struct Index {
    name: String,
}

fn main() {
    let app = tsukuyomi::app(|scope| {
        scope.route(tsukuyomi::route!("/:name").reply(|name| Index { name }));
    }).unwrap();

    tsukuyomi::server(app).run_forever().unwrap();
}
