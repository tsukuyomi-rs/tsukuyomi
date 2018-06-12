extern crate futures;
extern crate http;
extern crate pretty_env_logger;
extern crate tokio_io;
extern crate tsukuyomi;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;

mod lines;

use tsukuyomi::future::{ready, Ready};
use tsukuyomi::upgrade::Upgrade;
use tsukuyomi::{App, Context};

fn index(cx: &Context) -> Ready<tsukuyomi::Result<Upgrade>> {
    ready(lines::start(cx, |line| {
        if !line.is_empty() {
            Some(format!(">> {}", line))
        } else {
            None
        }
    }))
}

fn main() -> tsukuyomi::AppResult<()> {
    ::std::env::set_var("RUST_LOG", "lines=info");
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", index);
        })
        .finish()?;

    tsukuyomi::run(app)
}
