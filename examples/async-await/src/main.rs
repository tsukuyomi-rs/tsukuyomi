#![feature(proc_macro)]
#![feature(proc_macro_non_items)]
#![feature(generators)]

extern crate futures_await as futures;
extern crate tsukuyomi;

use futures::prelude::{async, await, Future};
use tsukuyomi::{App, Error, Handler, Input};

#[async]
fn async_handler() -> tsukuyomi::Result<String> {
    let read_all = Input::with_current(|input| input.body_mut().read_all());
    let body = await!(read_all.convert_to())?;
    println!("Received: {:?}", body);
    Ok(body)
}

fn async_handler_with_input(input: &mut Input) -> impl Future<Item = String, Error = Error> + Send + 'static {
    input.body_mut().read_all().convert_to().and_then(|body| {
        println!("Received: {:?}", body);
        Ok(body)
    })
}

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .mount("/", |m| {
            m.post("/async1").handle(Handler::new_fully_async(async_handler));
            m.post("/async2").handle(Handler::new_async(async_handler_with_input));
        })
        .finish()?;

    tsukuyomi::run(app)
}
