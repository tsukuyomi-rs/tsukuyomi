#![feature(proc_macro)]
#![feature(proc_macro_non_items, generators)] // for futures-await

extern crate futures_await as futures;
extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use futures::prelude::{await, Future};
use tsukuyomi::{App, Error, Input};
use tsukuyomi_codegen::handler;

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
            m.get("/ready").handle(ready_handler);
            m.post("/async").handle(async_handler);
            m.post("/await").handle(await_handler);
        })
        .finish()?;

    tsukuyomi::run(app)
}
