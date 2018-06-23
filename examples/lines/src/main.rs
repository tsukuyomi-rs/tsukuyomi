extern crate futures;
extern crate http;
extern crate pretty_env_logger;
extern crate tokio_codec;
extern crate tsukuyomi;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;

mod lines;

use tsukuyomi::output::Responder;
use tsukuyomi::{App, Handler, Input};

fn index(input: &mut Input) -> impl Responder {
    lines::start(input, |line| {
        if !line.is_empty() {
            Some(format!(">> {}", line))
        } else {
            None
        }
    })
}

fn main() -> tsukuyomi::AppResult<()> {
    ::std::env::set_var("RUST_LOG", "lines=info");
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/").handle(Handler::new_ready(index));
        })
        .finish()?;

    tsukuyomi::run(app)
}
