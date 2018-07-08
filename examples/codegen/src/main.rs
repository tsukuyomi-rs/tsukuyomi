#![feature(proc_macro)]
#![feature(proc_macro_non_items, generators)] // for futures-await

extern crate futures_await as futures;
extern crate tsukuyomi;

use futures::prelude::{await, Future};
use tsukuyomi::prelude::handler;
use tsukuyomi::{App, Error, Handler, Input};

#[handler]
fn ready_handler() -> &'static str {
    "Hello, Tsukuyomi.\n"
}

#[handler(async)]
fn async_handler(input: &mut Input) -> impl Future<Item = String, Error = Error> + Send + 'static {
    input.body_mut().read_all().convert_to()
}

#[handler(await)]
fn await_handler() -> tsukuyomi::Result<String> {
    let read_all = Input::with_current(|input| input.body_mut().read_all());
    let data: String = await!(read_all.convert_to())?;
    Ok(format!("Received: {}", data))
}

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |m| {
            m.get("/ready").handle(Handler::new(ready_handler));
            m.post("/async").handle(Handler::new(async_handler));
            m.post("/await").handle(Handler::new(await_handler));
        })
        .finish()?;

    tsukuyomi::run(app)
}
