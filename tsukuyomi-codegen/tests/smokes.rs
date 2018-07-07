#![feature(proc_macro, proc_macro_non_items, generators)]

extern crate futures_await as futures;
extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use futures::future;
use futures::prelude::{async, await, Future};
use tsukuyomi::{Error, Handler, Input};
use tsukuyomi_codegen::handler;

#[allow(dead_code)]
fn ready_without_input() {
    #[handler]
    fn handler() -> &'static str {
        "Hello"
    }
    let _ = Handler::new(handler);
}

#[allow(dead_code)]
fn ready_with_input() {
    #[handler]
    fn handler(_: &mut Input) -> &'static str {
        "Hello"
    }
    let _ = Handler::new(handler);
}

#[allow(dead_code)]
fn async_without_input() {
    #[handler(async)]
    fn handler() -> impl Future<Item = &'static str, Error = Error> + Send + 'static {
        future::ok("Hello")
    }
    let _ = Handler::new(handler);
}

#[allow(dead_code)]
fn async_with_input() {
    #[handler(async)]
    fn handler(_: &mut Input) -> impl Future<Item = &'static str, Error = Error> + Send + 'static {
        future::ok("Hello")
    }
    let _ = Handler::new(handler);
}

#[allow(dead_code)]
fn async_with_await() {
    #[handler(await)]
    fn handler() -> Result<&'static str, Error> {
        await!(future::ok::<(), Error>(()))?;
        Ok("Hello")
    }
    let _ = Handler::new(handler);
}
