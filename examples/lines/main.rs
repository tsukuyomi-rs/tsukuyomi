extern crate futures;
extern crate ganymede;
extern crate http;
extern crate pretty_env_logger;
extern crate tokio_io;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;

mod lines;

use ganymede::upgrade::Upgrade;
use ganymede::{App, Context};

fn index(cx: &Context) -> ganymede::Result<Upgrade> {
    lines::start(cx, |line| {
        if !line.is_empty() {
            Some(format!(">> {}", line))
        } else {
            None
        }
    })
}

fn main() -> ganymede::AppResult<()> {
    ::std::env::set_var("RUST_LOG", "lines=info");
    pretty_env_logger::init();

    let app = App::builder()
        .mount("/", |r| {
            r.get("/", index);
        })
        .finish()?;

    ganymede::run(app)
}
