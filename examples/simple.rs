extern crate ganymede;
extern crate http;
extern crate pretty_env_logger;

use ganymede::app::App;
use ganymede::context::Context;
use ganymede::error::Error;
use ganymede::response::ResponseBody;
use ganymede::router::{Route, Router, RouterContext};
use http::{Method, Response};

fn welcome(_cx: &Context, _rcx: &mut RouterContext) -> Result<Response<ResponseBody>, Error> {
    Ok(Response::builder().body(Default::default()).unwrap())
}

fn main() -> ganymede::rt::Result<()> {
    pretty_env_logger::init();

    let router = Router::builder()
        .mount(vec![Route::new("/", Method::GET, welcome)])
        .finish()?;

    App::new(router).serve()
}
