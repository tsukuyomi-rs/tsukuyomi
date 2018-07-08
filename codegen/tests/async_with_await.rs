#![feature(proc_macro)]
#![feature(proc_macro_non_items, generators)]

extern crate futures_await as futures;
extern crate tsukuyomi;
extern crate tsukuyomi_codegen;

use futures::future;
use futures::prelude::await;
use tsukuyomi::{Error, Handler};
use tsukuyomi_codegen::handler;

#[handler(await)]
fn handler() -> Result<&'static str, Error> {
    await!(future::ok::<(), Error>(()))?;
    Ok("Hello")
}

#[test]
fn main() {
    let _ = Handler::new(handler);
}
