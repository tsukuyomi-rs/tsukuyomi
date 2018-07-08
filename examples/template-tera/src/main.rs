extern crate tsukuyomi;
extern crate tsukuyomi_template;
#[macro_use]
extern crate tera;
#[macro_use]
extern crate serde;

use tsukuyomi::{App, Handler, Input};
use tsukuyomi_template::tera::Template;

#[derive(Debug, Serialize)]
struct Hello {
    name: String,
}

fn index(input: &mut Input) -> Template<Hello> {
    let name = input.params()[0].to_owned();
    Template::new("index.html", Hello { name: name })
}

fn main() -> tsukuyomi::AppResult<()> {
    let tera = compile_templates!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/**/*"));
    let app = App::builder()
        .manage(tera)
        .mount("/", |m| {
            m.get("/:name").handle(Handler::new_ready(index));
        })
        .finish()?;

    tsukuyomi::run(app)
}
