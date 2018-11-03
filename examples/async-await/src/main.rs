#![feature(use_extern_macros)]
#![feature(proc_macro_non_items)]
#![feature(generators)]

extern crate futures_await as futures;
extern crate tsukuyomi;

use futures::prelude::await;
use futures::prelude::*; // overwrite the built-in await!() macro

use tsukuyomi::input::body::ReadAll;
use tsukuyomi::{handler, input, App, AsyncResponder, Input};

// FIXME: implement as a static method of `ReadAll`
fn read_all() -> ReadAll {
    input::with_get_current(|input| input.body_mut().read_all())
}

#[async]
fn async_1() -> tsukuyomi::Result<String> {
    let body = await!(read_all().convert_to())?;
    println!("Received: {:?}", body);
    Ok(body)
}

fn async_2(input: &mut Input) -> impl AsyncResponder {
    input.body_mut().read_all().convert_to().and_then(|body: String| {
        println!("Received: {:?}", body);
        Ok(body)
    })
}

#[handler(async)]
fn async_3() -> tsukuyomi::Result<String> {
    let body = await!(read_all().convert_to())?;
    println!("Received: {:?}", body);
    Ok(body)
}

fn main() -> tsukuyomi::AppResult<()> {
    let app = App::builder()
        .route(("/async1", "POST", handler::wrap_async(|_| async_1())))
        .route(("/async2", "POST", handler::wrap_async(async_2)))
        .route(("/async3", "POST", async_3))
        .finish()?;

    tsukuyomi::run(app)
}
