extern crate tsukuyomi;
extern crate tsukuyomi_template;
#[macro_use]
extern crate askama;

use askama::Template as _Template;
use tsukuyomi::{App, Handler, Input};
use tsukuyomi_template::askama::Template;

#[derive(Debug, Template)]
#[template(path = "index.html")]
struct Hello {
    name: String,
}

fn index(input: &mut Input) -> Template<Hello> {
    let name = input.params()[0].to_owned();
    Template(Hello { name: name })
}

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |r| {
            r.get("/:name").handle(Handler::new_ready(index));
        })
        .finish()?;

    tsukuyomi::run(app)
}
