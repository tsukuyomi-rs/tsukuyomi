extern crate ganymede;
extern crate http;
extern crate pretty_env_logger;

use ganymede::app::App;
use ganymede::context::Context;
use ganymede::error::Error;
use ganymede::response::Output;
use ganymede::router::{Route, RouterContext};
use http::{Method, Response};

fn welcome(_cx: &Context, _rcx: &mut RouterContext) -> Result<Output, Error> {
    Ok(Response::builder().body("Hello").unwrap().into())
}

fn main() -> ganymede::rt::Result<()> {
    pretty_env_logger::init();
    App::builder()
        .mount(vec![Route::new("/", Method::GET, welcome)])
        .finish()?
        .serve()
}
